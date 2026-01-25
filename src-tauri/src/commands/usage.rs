use crate::db::{
    ConversationUsageSummary,
    Db,
    MessageUsage,
    SaveMessageUsageInput,
    UsageOperations,
    UsageStatistics,
    DbOperations,
};
use crate::events::{AgentEvent, EventBus, EVENT_MESSAGE_USAGE_SAVED, EVENT_USAGE_UPDATED};
use serde_json::json;
use serde::{Deserialize, Serialize};
use tauri::State;
use chrono::{TimeZone, Utc};
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize)]
struct PricingEntry {
    input: f64,
    output: f64,
    per: f64,
}

#[derive(Debug, Deserialize)]
struct PricingData {
    pricing: HashMap<String, PricingEntry>,
}

static PRICING: OnceLock<HashMap<String, PricingEntry>> = OnceLock::new();

fn get_pricing() -> &'static HashMap<String, PricingEntry> {
    PRICING.get_or_init(|| {
        let raw = include_str!("../../../src/lib/models/registry/pricing.json");
        let parsed: PricingData = serde_json::from_str(raw).unwrap_or(PricingData {
            pricing: HashMap::new(),
        });
        parsed.pricing
    })
}

fn calculate_estimated_cost(model: &str, prompt_tokens: i32, completion_tokens: i32) -> f64 {
    let pricing = get_pricing();
    let entry = pricing
        .get(model)
        .or_else(|| {
            let cleaned = model
                .split(|c| c == ' ' || c == '•' || c == '/' || c == ':')
                .last()
                .unwrap_or(model);
            pricing.get(cleaned)
        })
        .or_else(|| pricing.iter().find(|(key, _)| model.contains(*key)).map(|(_, v)| v));

    let entry = match entry {
        Some(entry) => entry,
        None => return 0.0,
    };

    let prompt = prompt_tokens.max(0) as f64;
    let completion = completion_tokens.max(0) as f64;
    if entry.per <= 0.0 {
        return 0.0;
    }

    let input_cost = (prompt / entry.per) * entry.input;
    let output_cost = (completion / entry.per) * entry.output;
    let total = input_cost + output_cost;

    (total * 1_000_000.0).round() / 1_000_000.0
}

fn estimate_tokens(text: &str) -> i32 {
    let chars = text.chars().count() as f64;
    let estimate = (chars * 0.25).ceil() as i32;
    estimate.max(0)
}

#[derive(Debug, Serialize)]
pub struct UsageBackfillResult {
    pub conversations_scanned: usize,
    pub messages_checked: usize,
    pub messages_backfilled: usize,
    pub conversations_updated: usize,
    pub fallback_model_used: usize,
}

#[tauri::command]
pub fn save_message_usage(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    input: SaveMessageUsageInput
) -> Result<MessageUsage, String> {
    let usage = UsageOperations::save_message_usage(&*state, input)
        .map_err(|e| e.to_string())?;

    let timestamp_ms = usage.created_at.timestamp_millis();
    event_bus.publish(AgentEvent::new_with_timestamp(
        EVENT_MESSAGE_USAGE_SAVED,
        json!({
            "id": usage.id,
            "message_id": usage.message_id,
            "model_name": usage.model_name,
            "prompt_tokens": usage.prompt_tokens,
            "completion_tokens": usage.completion_tokens,
            "total_tokens": usage.total_tokens,
            "estimated_cost": usage.estimated_cost,
            "timestamp_ms": timestamp_ms
        }),
        timestamp_ms,
    ));

    Ok(usage)
}

#[tauri::command]
pub fn update_conversation_usage(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    conversation_id: String
) -> Result<ConversationUsageSummary, String> {
    let summary = UsageOperations::update_conversation_usage(&*state, &conversation_id)
        .map_err(|e| e.to_string())?;

    let timestamp_ms = summary.last_updated.timestamp_millis();
    event_bus.publish(AgentEvent::new_with_timestamp(
        EVENT_USAGE_UPDATED,
        json!({
            "conversation_id": summary.conversation_id,
            "total_prompt_tokens": summary.total_prompt_tokens,
            "total_completion_tokens": summary.total_completion_tokens,
            "total_tokens": summary.total_tokens,
            "total_cost": summary.total_cost,
            "message_count": summary.message_count,
            "timestamp_ms": timestamp_ms
        }),
        timestamp_ms,
    ));

    Ok(summary)
}

#[tauri::command]
pub fn get_conversation_usage(
    state: State<'_, Db>,
    conversation_id: String
) -> Result<Option<ConversationUsageSummary>, String> {
    UsageOperations::get_conversation_usage(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_usage_statistics(
    state: State<'_, Db>,
    start_date: Option<i64>,
    end_date: Option<i64>
) -> Result<UsageStatistics, String> {
    // Convert Option<i64> timestamps to Option<DateTime<Utc>>
    let start_date_time = start_date.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
    let end_date_time = end_date.and_then(|ts| Utc.timestamp_opt(ts, 0).single());

    UsageOperations::get_usage_statistics(&*state, start_date_time, end_date_time)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_message_usage(
    state: State<'_, Db>,
    message_id: String
) -> Result<Option<MessageUsage>, String> {
    UsageOperations::get_message_usage(&*state, &message_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn backfill_message_usage(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    conversation_id: Option<String>,
    default_model: Option<String>,
    dry_run: Option<bool>
) -> Result<UsageBackfillResult, String> {
    let dry_run = dry_run.unwrap_or(false);
    let default_model = default_model
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty());

    let conversation_ids: Vec<String> = {
        let binding = state.conn();
        let conn = binding.lock().unwrap();
        let ids = if let Some(target) = conversation_id {
            vec![target]
        } else {
            let mut stmt = conn
                .prepare("SELECT id FROM conversations")
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |row| row.get(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<String>, _>>()
                .map_err(|e| e.to_string())?
        };
        ids
    };

    let mut result = UsageBackfillResult {
        conversations_scanned: conversation_ids.len(),
        messages_checked: 0,
        messages_backfilled: 0,
        conversations_updated: 0,
        fallback_model_used: 0,
    };

    for conversation_id in conversation_ids {
        let (messages, usage_map, latest_model) = {
            let binding = state.conn();
            let conn = binding.lock().unwrap();

            let mut message_stmt = conn
                .prepare(
                    "SELECT id, role, content, created_at
                     FROM messages
                     WHERE conversation_id = ?1
                     ORDER BY created_at ASC",
                )
                .map_err(|e| e.to_string())?;

            let messages = message_stmt
                .query_map(params![conversation_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            let mut usage_stmt = conn
                .prepare(
                    "SELECT mu.message_id, mu.model_name
                     FROM message_usage mu
                     JOIN messages m ON m.id = mu.message_id
                     WHERE m.conversation_id = ?1",
                )
                .map_err(|e| e.to_string())?;

            let usage_map = usage_stmt
                .query_map(params![conversation_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<HashMap<String, String>, _>>()
                .map_err(|e| e.to_string())?;

            let latest_model = conn
                .query_row(
                    "SELECT mu.model_name
                     FROM message_usage mu
                     JOIN messages m ON m.id = mu.message_id
                     WHERE m.conversation_id = ?1
                     ORDER BY mu.created_at DESC
                     LIMIT 1",
                    params![conversation_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            (messages, usage_map, latest_model)
        };

        let usage_map_empty = usage_map.is_empty();
        let mut existing_usage: HashSet<String> = usage_map.keys().cloned().collect();
        let mut current_model = latest_model.or_else(|| default_model.clone());
        if current_model.is_none() {
            current_model = Some("unknown".to_string());
        }

        let mut running_prompt_tokens: i32 = 0;
        let mut conversation_backfilled = 0;
        for (message_id, role, content, created_at) in messages {
            let estimated_tokens = estimate_tokens(&content);
            if let Some(model_name) = usage_map.get(&message_id) {
                current_model = Some(model_name.clone());
            }

            if role == "assistant" {
                result.messages_checked += 1;
                if !existing_usage.contains(&message_id) {
                    let model_name = current_model.clone().unwrap_or_else(|| "unknown".to_string());

                    let prompt_tokens = running_prompt_tokens.max(0);
                    let completion_tokens = estimated_tokens.max(0);
                    let total_tokens = prompt_tokens + completion_tokens;
                    let estimated_cost = calculate_estimated_cost(
                        &model_name,
                        prompt_tokens,
                        completion_tokens,
                    );

                    if !dry_run {
                        let binding = state.conn();
                        let conn = binding.lock().unwrap();
                        conn.execute(
                            "INSERT INTO message_usage (id, message_id, model_name, prompt_tokens, completion_tokens, total_tokens, estimated_cost, created_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            params![
                                Uuid::new_v4().to_string(),
                                message_id,
                                model_name,
                                prompt_tokens,
                                completion_tokens,
                                total_tokens,
                                estimated_cost,
                                created_at
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                    }

                    existing_usage.insert(message_id);
                    conversation_backfilled += 1;
                    result.messages_backfilled += 1;
                }
            }

            running_prompt_tokens = running_prompt_tokens.saturating_add(estimated_tokens);
        }

        if conversation_backfilled > 0 && usage_map_empty {
            result.fallback_model_used += 1;
        }

        if conversation_backfilled > 0 && !dry_run {
            if let Ok(summary) = UsageOperations::update_conversation_usage(&*state, &conversation_id) {
                let timestamp_ms = summary.last_updated.timestamp_millis();
                event_bus.publish(AgentEvent::new_with_timestamp(
                    EVENT_USAGE_UPDATED,
                    json!({
                        "conversation_id": summary.conversation_id,
                        "total_prompt_tokens": summary.total_prompt_tokens,
                        "total_completion_tokens": summary.total_completion_tokens,
                        "total_tokens": summary.total_tokens,
                        "total_cost": summary.total_cost,
                        "message_count": summary.message_count,
                        "timestamp_ms": timestamp_ms
                    }),
                    timestamp_ms,
                ));
            }

            result.conversations_updated += 1;
        }
    }

    Ok(result)
}
