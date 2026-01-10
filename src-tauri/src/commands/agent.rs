use crate::db::{
    BranchOperations,
    ConversationOperations,
    CustomBackendOperations,
    Db,
    IncomingAttachment,
    MessageOperations,
    ModelOperations,
    SaveMessageUsageInput,
    UsageOperations,
};
use crate::events::{
    AgentEvent,
    EventBus,
    EVENT_ASSISTANT_STREAM_CHUNK,
    EVENT_ASSISTANT_STREAM_COMPLETED,
    EVENT_ASSISTANT_STREAM_STARTED,
    EVENT_CONVERSATION_UPDATED,
    EVENT_MESSAGE_SAVED,
    EVENT_MESSAGE_USAGE_SAVED,
    EVENT_USAGE_UPDATED,
};
use crate::llm::{
    complete_anthropic,
    complete_openai,
    complete_openai_compatible,
    stream_anthropic,
    stream_ndjson,
    stream_openai,
    stream_openai_compatible,
    stream_openai_compatible_response,
    LlmMessage,
    Usage,
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::OnceLock;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;
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

fn value_to_string(value: &serde_json::Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(array) = value.as_array() {
        let mut combined = String::new();
        for entry in array {
            if let Some(text) = entry.get("text").and_then(|v| v.as_str()) {
                combined.push_str(text);
            }
        }
        return combined;
    }

    value.to_string()
}

fn estimate_tokens(text: &str) -> i32 {
    let chars = text.chars().count() as f64;
    let estimate = (chars * 0.25).ceil() as i32;
    estimate.max(0)
}

fn estimate_prompt_tokens(messages: &[LlmMessage]) -> i32 {
    messages
        .iter()
        .map(|message| estimate_tokens(&value_to_string(&message.content)))
        .sum()
}

#[derive(Debug, Deserialize)]
pub struct AgentSendMessagePayload {
    pub conversation_id: Option<String>,
    pub model: String,
    pub provider: String,
    pub system_prompt: Option<String>,
    pub content: String,
    pub attachments: Vec<IncomingAttachment>,
    pub user_message_id: Option<String>,
    pub assistant_message_id: Option<String>,
    pub custom_backend_id: Option<String>,
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AgentSendMessageResult {
    pub conversation_id: String,
    pub user_message_id: String,
    pub assistant_message_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AgentGenerateTitlePayload {
    pub conversation_id: String,
    pub model: String,
    pub provider: String,
    pub custom_backend_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentGenerateTitleResult {
    pub title: String,
}

#[tauri::command(rename_all = "snake_case")]
pub fn agent_send_message(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    payload: AgentSendMessagePayload,
) -> Result<AgentSendMessageResult, String> {
    let AgentSendMessagePayload {
        conversation_id,
        model,
        provider,
        system_prompt,
        content,
        attachments,
        user_message_id,
        assistant_message_id,
        custom_backend_id,
        stream,
    } = payload;

    let conversation_id = conversation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    ConversationOperations::get_or_create_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())?;

    let user_message_id = MessageOperations::save_message(
        &*state,
        &conversation_id,
        "user",
        &content,
        &attachments,
        user_message_id,
    )
    .map_err(|e| e.to_string())?;

    let timestamp_ms = Utc::now().timestamp_millis();
    event_bus.publish(AgentEvent::new_with_timestamp(
        EVENT_MESSAGE_SAVED,
        json!({
            "conversation_id": conversation_id,
            "message_id": user_message_id,
            "role": "user",
            "content": content,
            "attachments": attachments,
            "timestamp_ms": timestamp_ms
        }),
        timestamp_ms,
    ));

    let assistant_message_id = assistant_message_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let history = MessageOperations::get_messages(&*state, &conversation_id)
        .map_err(|e| e.to_string())?;

    let main_branch = BranchOperations::get_or_create_main_branch(&*state, &conversation_id)
        .map_err(|e| e.to_string())?;

    let parent_message_id = history
        .iter()
        .rev()
        .skip(1)
        .find(|message| message.id != user_message_id)
        .map(|message| message.id.clone());

    let _ = BranchOperations::create_message_tree_node(
        &*state,
        &user_message_id,
        parent_message_id.as_deref(),
        &main_branch.id,
        false,
    );

    let mut messages: Vec<LlmMessage> = Vec::new();
    if let Some(system_prompt) = system_prompt.clone() {
        if !system_prompt.trim().is_empty() {
            messages.push(LlmMessage {
                role: "system".to_string(),
                content: json!(system_prompt),
            });
        }
    }

    for message in history {
        let content = if message.id == user_message_id {
            build_user_content(&message.content, &attachments)
        } else {
            json!(message.content)
        };

        messages.push(LlmMessage {
            role: message.role,
            content,
        });
    }

    let provider = provider.to_lowercase();
    let model = model.clone();
    let stream_enabled = stream.unwrap_or(true);

    match provider.as_str() {
        "openai" | "anthropic" | "deepseek" => {
            let api_key = ModelOperations::get_api_key(&*state, &provider)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if api_key.is_empty() {
                return Err(format!("Missing API key for provider: {provider}"));
            }
        }
        "custom" => {
            let backend_id = custom_backend_id.clone().ok_or_else(|| {
                "Custom provider requires custom_backend_id".to_string()
            })?;
            let backend = CustomBackendOperations::get_custom_backend_by_id(&*state, &backend_id)
                .map_err(|e| e.to_string())?;
            if backend.is_none() {
                return Err("Custom backend not found".to_string());
            }
        }
        "ollama" => {}
        _ => {}
    }

    let db = state.inner().clone();
    let bus = event_bus.inner().clone();
    let custom_backend_id = custom_backend_id.clone();
    let system_prompt_for_thread = system_prompt.clone();
    let conversation_id_for_thread = conversation_id.clone();
    let assistant_message_id_for_thread = assistant_message_id.clone();
    let model_for_thread = model.clone();
    let main_branch_id_for_thread = main_branch.id.clone();
    let user_message_id_for_thread = user_message_id.clone();

    std::thread::spawn(move || {
        let stream_timestamp = Utc::now().timestamp_millis();
        bus.publish(AgentEvent::new_with_timestamp(
            EVENT_ASSISTANT_STREAM_STARTED,
            json!({
                "conversation_id": conversation_id_for_thread,
                "message_id": assistant_message_id_for_thread,
                "timestamp_ms": stream_timestamp
            }),
            stream_timestamp,
        ));

        let mut accumulated = String::new();
        let mut usage: Option<Usage> = None;
        let client = Client::new();

        let result = match provider.as_str() {
            "openai" => {
                let api_key = ModelOperations::get_api_key(&db, "openai")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                if api_key.is_empty() {
                    Err("Missing OpenAI API key".to_string())
                } else if !stream_enabled {
                    complete_openai(
                        &client,
                        &api_key,
                        "https://api.openai.com/v1/chat/completions",
                        &model_for_thread,
                        &messages,
                    )
                } else {
                    stream_openai(
                        &client,
                        &api_key,
                        "https://api.openai.com/v1/chat/completions",
                        &model_for_thread,
                        &messages,
                        |chunk| {
                            accumulated.push_str(chunk);
                            let timestamp_ms = Utc::now().timestamp_millis();
                            bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_ASSISTANT_STREAM_CHUNK,
                                json!({
                                    "conversation_id": conversation_id_for_thread,
                                    "message_id": assistant_message_id_for_thread,
                                    "chunk": chunk,
                                    "timestamp_ms": timestamp_ms
                                }),
                                timestamp_ms,
                            ));
                        },
                    )
                }
            }
            "anthropic" => {
                let api_key = ModelOperations::get_api_key(&db, "anthropic")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                if api_key.is_empty() {
                    Err("Missing Anthropic API key".to_string())
                } else if !stream_enabled {
                    complete_anthropic(
                        &client,
                        &api_key,
                        &model_for_thread,
                        system_prompt_for_thread.as_deref(),
                        &messages,
                    )
                } else {
                    stream_anthropic(
                        &client,
                        &api_key,
                        &model_for_thread,
                        system_prompt_for_thread.as_deref(),
                        &messages,
                        |chunk| {
                            accumulated.push_str(chunk);
                            let timestamp_ms = Utc::now().timestamp_millis();
                            bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_ASSISTANT_STREAM_CHUNK,
                                json!({
                                    "conversation_id": conversation_id_for_thread,
                                    "message_id": assistant_message_id_for_thread,
                                    "chunk": chunk,
                                    "timestamp_ms": timestamp_ms
                                }),
                                timestamp_ms,
                            ));
                        },
                    )
                }
            }
            "deepseek" => {
                let api_key = ModelOperations::get_api_key(&db, "deepseek")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                if api_key.is_empty() {
                    Err("Missing DeepSeek API key".to_string())
                } else if !stream_enabled {
                    complete_openai_compatible(
                        &client,
                        Some(&api_key),
                        "https://api.deepseek.com/chat/completions",
                        &model_for_thread,
                        &messages,
                    )
                } else {
                    stream_openai_compatible(
                        &client,
                        Some(&api_key),
                        "https://api.deepseek.com/chat/completions",
                        &model_for_thread,
                        &messages,
                        None,
                        |chunk| {
                            accumulated.push_str(chunk);
                            let timestamp_ms = Utc::now().timestamp_millis();
                            bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_ASSISTANT_STREAM_CHUNK,
                                json!({
                                    "conversation_id": conversation_id_for_thread,
                                    "message_id": assistant_message_id_for_thread,
                                    "chunk": chunk,
                                    "timestamp_ms": timestamp_ms
                                }),
                                timestamp_ms,
                            ));
                        },
                    )
                }
            }
            "custom" | "ollama" => {
                let (url, api_key) = match custom_backend_id {
                    Some(ref id) => {
                        let backend = CustomBackendOperations::get_custom_backend_by_id(&db, id)
                            .ok()
                            .flatten();
                        if let Some(backend) = backend {
                            (backend.url, backend.api_key)
                        } else {
                            (String::new(), None)
                        }
                    }
                    None => {
                        if provider == "ollama" {
                            ("http://localhost:11434/v1/chat/completions".to_string(), None)
                        } else {
                            (String::new(), None)
                        }
                    }
                };

                if url.is_empty() {
                    Err("Missing custom backend URL".to_string())
                } else if !stream_enabled {
                    complete_openai_compatible(
                        &client,
                        api_key.as_deref(),
                        &url,
                        &model_for_thread,
                        &messages,
                    )
                } else {
                    let response_result = {
                        let mut request = client
                            .post(&url)
                            .header("Content-Type", "application/json")
                            .json(&json!({
                                "model": model_for_thread,
                                "messages": messages,
                                "stream": true
                            }));
                        if let Some(key) = api_key.as_deref() {
                            request = request.bearer_auth(key);
                        }
                        request.send().map_err(|e| e.to_string())
                    };

                    match response_result {
                        Ok(response) => {
                            let is_sse = response
                                .headers()
                                .get("content-type")
                                .and_then(|v| v.to_str().ok())
                                .map(|v| v.contains("text/event-stream"))
                                .unwrap_or(false);

                            if is_sse {
                                stream_openai_compatible_response(response, |chunk| {
                                    accumulated.push_str(chunk);
                                    let timestamp_ms = Utc::now().timestamp_millis();
                                    bus.publish(AgentEvent::new_with_timestamp(
                                        EVENT_ASSISTANT_STREAM_CHUNK,
                                        json!({
                                            "conversation_id": conversation_id_for_thread,
                                            "message_id": assistant_message_id_for_thread,
                                            "chunk": chunk,
                                            "timestamp_ms": timestamp_ms
                                        }),
                                        timestamp_ms,
                                    ));
                                })
                            } else {
                                stream_ndjson(response, |chunk| {
                                    accumulated.push_str(chunk);
                                    let timestamp_ms = Utc::now().timestamp_millis();
                                    bus.publish(AgentEvent::new_with_timestamp(
                                        EVENT_ASSISTANT_STREAM_CHUNK,
                                        json!({
                                            "conversation_id": conversation_id_for_thread,
                                            "message_id": assistant_message_id_for_thread,
                                            "chunk": chunk,
                                            "timestamp_ms": timestamp_ms
                                        }),
                                        timestamp_ms,
                                    ));
                                })
                            }
                        }
                        Err(err) => Err(err),
                    }
                }
            }
            _ => Err(format!("Unsupported provider: {provider}")),
        };

        if let Ok(stream_result) = result {
            accumulated = stream_result.content;
            usage = stream_result.usage;
        }

        if !accumulated.is_empty() {
            let _ = MessageOperations::save_message(
                &db,
                &conversation_id_for_thread,
                "assistant",
                &accumulated,
                &[],
                Some(assistant_message_id_for_thread.clone()),
            );

            let _ = BranchOperations::create_message_tree_node(
                &db,
                &assistant_message_id_for_thread,
                Some(&user_message_id_for_thread),
                &main_branch_id_for_thread,
                false,
            );

            let timestamp_ms = Utc::now().timestamp_millis();
            bus.publish(AgentEvent::new_with_timestamp(
                EVENT_MESSAGE_SAVED,
                json!({
                    "conversation_id": conversation_id_for_thread,
                    "message_id": assistant_message_id_for_thread,
                    "role": "assistant",
                    "content": accumulated,
                    "attachments": [],
                    "timestamp_ms": timestamp_ms
                }),
                timestamp_ms,
            ));
        }

        let usage = usage
            .filter(|u| u.prompt_tokens > 0 || u.completion_tokens > 0)
            .or_else(|| {
                if accumulated.is_empty() {
                    None
                } else {
                    Some(Usage {
                        prompt_tokens: estimate_prompt_tokens(&messages),
                        completion_tokens: estimate_tokens(&accumulated),
                    })
                }
            });

        if let Some(usage) = usage {
            let estimated_cost = calculate_estimated_cost(
                &model_for_thread,
                usage.prompt_tokens,
                usage.completion_tokens,
            );
            let save_usage = SaveMessageUsageInput {
                message_id: assistant_message_id_for_thread.clone(),
                model_name: model_for_thread.clone(),
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                estimated_cost,
            };

            if let Ok(saved_usage) = UsageOperations::save_message_usage(&db, save_usage) {
                let timestamp_ms = saved_usage.created_at.timestamp_millis();
                bus.publish(AgentEvent::new_with_timestamp(
                    EVENT_MESSAGE_USAGE_SAVED,
                    json!({
                        "id": saved_usage.id,
                        "message_id": saved_usage.message_id,
                        "model_name": saved_usage.model_name,
                        "prompt_tokens": saved_usage.prompt_tokens,
                        "completion_tokens": saved_usage.completion_tokens,
                        "total_tokens": saved_usage.total_tokens,
                        "estimated_cost": saved_usage.estimated_cost,
                        "timestamp_ms": timestamp_ms
                    }),
                    timestamp_ms,
                ));
            }

            if let Ok(summary) = UsageOperations::update_conversation_usage(&db, &conversation_id_for_thread) {
                let timestamp_ms = summary.last_updated.timestamp_millis();
                bus.publish(AgentEvent::new_with_timestamp(
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
        }

        let timestamp_ms = Utc::now().timestamp_millis();
        bus.publish(AgentEvent::new_with_timestamp(
            EVENT_ASSISTANT_STREAM_COMPLETED,
            json!({
                "conversation_id": conversation_id_for_thread,
                "message_id": assistant_message_id_for_thread,
                "content": accumulated,
                "timestamp_ms": timestamp_ms
            }),
            timestamp_ms,
        ));
    });

    Ok(AgentSendMessageResult {
        conversation_id,
        user_message_id,
        assistant_message_id,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn agent_generate_title(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    payload: AgentGenerateTitlePayload,
) -> Result<AgentGenerateTitleResult, String> {
    let provider = payload.provider.to_lowercase();
    let model = payload.model.clone();

    let history = MessageOperations::get_messages(&*state, &payload.conversation_id)
        .map_err(|e| e.to_string())?;

    let first_user_message = history
        .iter()
        .find(|message| message.role == "user")
        .map(|message| message.content.clone())
        .unwrap_or_else(|| "New Conversation".to_string());

    let system_prompt = "You are a helpful assistant that generates short, descriptive titles for conversations. \
Generate a concise title (maximum 5 words) that captures the main topic or intent of the conversation. \
Respond ONLY with the title, no quotes, no explanation, no punctuation at the end.";

    let user_prompt = format!(
        "Generate a short title for a conversation that starts with this message: \"{}\"",
        first_user_message
    );

    let messages = vec![
        LlmMessage {
            role: "system".to_string(),
            content: json!(system_prompt),
        },
        LlmMessage {
            role: "user".to_string(),
            content: json!(user_prompt),
        },
    ];

    let client = Client::new();
    let mut title = match provider.as_str() {
        "openai" => {
            let api_key = ModelOperations::get_api_key(&*state, "openai")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if api_key.is_empty() {
                return Err("Missing OpenAI API key".to_string());
            }
            complete_openai(
                &client,
                &api_key,
                "https://api.openai.com/v1/chat/completions",
                &model,
                &messages,
            )
        }
        "anthropic" => {
            let api_key = ModelOperations::get_api_key(&*state, "anthropic")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if api_key.is_empty() {
                return Err("Missing Anthropic API key".to_string());
            }
            complete_anthropic(&client, &api_key, &model, Some(system_prompt), &messages)
        }
        "deepseek" => {
            let api_key = ModelOperations::get_api_key(&*state, "deepseek")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if api_key.is_empty() {
                return Err("Missing DeepSeek API key".to_string());
            }
            complete_openai_compatible(
                &client,
                Some(&api_key),
                "https://api.deepseek.com/chat/completions",
                &model,
                &messages,
            )
        }
        "custom" | "ollama" => {
            let (url, api_key) = if provider == "ollama" {
                ("http://localhost:11434/v1/chat/completions".to_string(), None)
            } else {
                let backend_id = payload.custom_backend_id.clone().ok_or_else(|| {
                    "Custom provider requires custom_backend_id".to_string()
                })?;
                let backend = CustomBackendOperations::get_custom_backend_by_id(&*state, &backend_id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "Custom backend not found".to_string())?;
                (backend.url, backend.api_key)
            };

            complete_openai_compatible(&client, api_key.as_deref(), &url, &model, &messages)
        }
        _ => return Err(format!("Unsupported provider: {}", provider)),
    }
    .map_err(|e| e.to_string())?
    .content;

    title = title.trim().trim_matches('"').trim_matches('\'').to_string();
    if title.is_empty() {
        title = "New Conversation".to_string();
    }

    ConversationOperations::update_conversation_name(&*state, &payload.conversation_id, &title)
        .map_err(|e| e.to_string())?;

    let timestamp_ms = Utc::now().timestamp_millis();
    event_bus.publish(AgentEvent::new_with_timestamp(
        EVENT_CONVERSATION_UPDATED,
        json!({
            "conversation_id": payload.conversation_id,
            "name": title,
            "timestamp_ms": timestamp_ms
        }),
        timestamp_ms,
    ));

    Ok(AgentGenerateTitleResult { title })
}

fn build_user_content(content: &str, attachments: &[IncomingAttachment]) -> serde_json::Value {
    if attachments.is_empty() {
        return json!(content);
    }

    let mut text = content.to_string();
    let mut image_entries = Vec::new();
    let mut image_names = Vec::new();

    for attachment in attachments {
        let attachment_type = attachment.attachment_type.as_str();
        if attachment_type.starts_with("text") || attachment_type.starts_with("application/json") {
            text.push_str(&format!(
                "\n\n[Attached file: {}]\n```\n{}\n```\n",
                attachment.name, attachment.data
            ));
        } else if attachment_type.starts_with("image") {
            image_names.push(attachment.name.clone());
            image_entries.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": attachment.data,
                    "detail": "auto"
                }
            }));
        }
    }

    if !image_names.is_empty() {
        text.push_str(&format!(
            "\n\n[Attached images: {}]",
            image_names.join(", ")
        ));
    }

    let mut content_array = vec![json!({ "type": "text", "text": text })];
    content_array.extend(image_entries);

    json!(content_array)
}
