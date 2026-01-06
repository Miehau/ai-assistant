use crate::db::{
    ConversationUsageSummary,
    Db,
    MessageUsage,
    SaveMessageUsageInput,
    UsageOperations,
    UsageStatistics,
};
use crate::events::{AgentEvent, EventBus, EVENT_MESSAGE_USAGE_SAVED, EVENT_USAGE_UPDATED};
use serde_json::json;
use tauri::State;
use chrono::{TimeZone, Utc};

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
