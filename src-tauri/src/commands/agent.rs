use crate::db::{
    BranchOperations,
    ConversationOperations,
    CustomBackendOperations,
    Db,
    IncomingAttachment,
    MessageOperations,
    MessageToolExecutionInput,
    ModelOperations,
    SaveMessageUsageInput,
    UsageOperations,
};
use crate::agent::{capture_topic_note, DynamicController};
use crate::agent::prompts::RESPONDER_PROMPT;
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
    complete_anthropic_with_output_format,
    complete_claude_cli,
    complete_openai,
    complete_openai_compatible,
    stream_anthropic,
    stream_openai,
    stream_openai_compatible,
    LlmMessage,
    Usage,
};
use crate::tools::{ApprovalStore, ToolRegistry};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
    let normalized_model = model.replace("claude-cli-", "claude-");
    let entry = pricing
        .get(normalized_model.as_str())
        .or_else(|| {
            let cleaned = normalized_model
                .split(|c| c == ' ' || c == '•' || c == '/' || c == ':')
                .last()
                .unwrap_or(normalized_model.as_str());
            pricing.get(cleaned)
        })
        .or_else(|| {
            pricing
                .iter()
                .find(|(key, _)| normalized_model.contains(*key))
                .map(|(_, v)| v)
        });

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

fn stream_response_chunks(
    bus: &EventBus,
    conversation_id: &str,
    message_id: &str,
    content: &str,
) {
    if content.is_empty() {
        return;
    }

    let chunk_size = if content.len() > 8000 { 200 } else { 120 };
    let total_chunks = (content.len() + chunk_size - 1) / chunk_size;
    let sleep_ms = if total_chunks <= 1 {
        0
    } else if total_chunks <= 30 {
        16
    } else if total_chunks <= 80 {
        8
    } else {
        4
    };

    let mut chunk = String::new();
    for ch in content.chars() {
        chunk.push(ch);
        if chunk.len() >= chunk_size {
            let timestamp_ms = Utc::now().timestamp_millis();
            bus.publish(AgentEvent::new_with_timestamp(
                EVENT_ASSISTANT_STREAM_CHUNK,
                json!({
                    "conversation_id": conversation_id,
                    "message_id": message_id,
                    "chunk": chunk,
                    "timestamp_ms": timestamp_ms
                }),
                timestamp_ms,
            ));
            chunk = String::new();
            if sleep_ms > 0 {
                std::thread::sleep(Duration::from_millis(sleep_ms));
            }
        }
    }

    if !chunk.is_empty() {
        let timestamp_ms = Utc::now().timestamp_millis();
        bus.publish(AgentEvent::new_with_timestamp(
            EVENT_ASSISTANT_STREAM_CHUNK,
            json!({
                "conversation_id": conversation_id,
                "message_id": message_id,
                "chunk": chunk,
                "timestamp_ms": timestamp_ms
            }),
            timestamp_ms,
        ));
    }
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
    tool_registry: State<'_, ToolRegistry>,
    approvals: State<'_, ApprovalStore>,
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
        stream: _stream,
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
        "ollama" | "claude_cli" => {}
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
    let tool_registry_for_thread = tool_registry.inner().clone();
    let approvals_for_thread = approvals.inner().clone();

    std::thread::spawn(move || {
        let client = Client::new();
        let mut draft = String::new();
        let mut usage_accumulator = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };

        let custom_backend_config = if provider == "custom" {
            custom_backend_id
                .as_ref()
                .and_then(|id| CustomBackendOperations::get_custom_backend_by_id(&db, id).ok())
                .flatten()
                .map(|backend| (backend.url, backend.api_key))
        } else if provider == "ollama" {
            Some(("http://localhost:11434/v1/chat/completions".to_string(), None))
        } else {
            None
        };

        let messages_for_usage = messages.clone();

        let mut tool_execution_inputs: Vec<MessageToolExecutionInput> = Vec::new();
        let mut call_llm =
            |messages: &[LlmMessage], system_prompt: Option<&str>, output_format: Option<Value>| {
                let prepared_messages = if provider == "anthropic" || provider == "claude_cli" {
                    messages.to_vec()
                } else {
                    let mut prepared = messages.to_vec();
                    if let Some(system_prompt) = system_prompt {
                        if !system_prompt.trim().is_empty() {
                            prepared.insert(
                                0,
                                LlmMessage {
                                    role: "system".to_string(),
                                    content: json!(system_prompt),
                                },
                            );
                        }
                    }
                    prepared
                };

                let result = match provider.as_str() {
                    "openai" => {
                        let api_key = ModelOperations::get_api_key(&db, "openai")
                            .ok()
                            .flatten()
                            .unwrap_or_default();
                        if api_key.is_empty() {
                            Err("Missing OpenAI API key".to_string())
                        } else {
                            complete_openai(
                                &client,
                                &api_key,
                                "https://api.openai.com/v1/chat/completions",
                                &model_for_thread,
                                &prepared_messages,
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
                        } else {
                            complete_anthropic_with_output_format(
                                &client,
                                &api_key,
                                &model_for_thread,
                                system_prompt,
                                &prepared_messages,
                                output_format,
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
                        } else {
                            complete_openai_compatible(
                                &client,
                                Some(&api_key),
                                "https://api.deepseek.com/chat/completions",
                                &model_for_thread,
                                &prepared_messages,
                            )
                        }
                    }
                    "claude_cli" => complete_claude_cli(
                        &model_for_thread,
                        system_prompt,
                        &prepared_messages,
                        output_format,
                    ),
                    "custom" | "ollama" => {
                        let (url, api_key) = custom_backend_config.clone().unwrap_or_default();
                        if url.is_empty() {
                            Err("Missing custom backend URL".to_string())
                        } else {
                            complete_openai_compatible(
                                &client,
                                api_key.as_deref(),
                                &url,
                                &model_for_thread,
                                &prepared_messages,
                            )
                        }
                    }
                    _ => Err(format!("Unsupported provider: {provider}")),
                };

                if let Ok(ref stream_result) = result {
                    if let Some(usage) = stream_result.usage.as_ref() {
                        usage_accumulator.prompt_tokens += usage.prompt_tokens;
                        usage_accumulator.completion_tokens += usage.completion_tokens;
                    } else {
                        usage_accumulator.prompt_tokens += estimate_prompt_tokens(&prepared_messages);
                        usage_accumulator.completion_tokens += estimate_tokens(&stream_result.content);
                    }
                }

                result
            };

        let mut controller_ok = false;
        let mut controller = match DynamicController::new(
            db.clone(),
            bus.clone(),
            tool_registry_for_thread.clone(),
            approvals_for_thread.clone(),
            messages,
            system_prompt_for_thread.clone(),
            conversation_id_for_thread.clone(),
            user_message_id_for_thread.clone(),
            assistant_message_id_for_thread.clone(),
        ) {
            Ok(controller) => Some(controller),
            Err(error) => {
                draft = format!("Agent setup error: {}", error);
                None
            }
        };

        if let Some(ref mut controller) = controller {
            match controller.run(&content, &mut call_llm) {
                Ok(response) => {
                    draft = response;
                    controller_ok = true;
                }
                Err(error) => {
                    draft = format!("Agent error: {}", error);
                }
            }
            tool_execution_inputs = controller.take_tool_executions();
        }

        if draft.trim().is_empty() && !tool_execution_inputs.is_empty() {
            draft = "Tool results available below.".to_string();
        }

        let mut final_response = draft.clone();
        let mut stream_started = false;

        let stream_supported = supports_streaming(&provider);
        let use_responder = controller_ok && stream_supported;

        if use_responder {
            let responder_prompt = build_responder_prompt(
                &content,
                &messages_for_usage,
                &tool_execution_inputs,
                &draft,
            );

            let responder_messages = vec![LlmMessage {
                role: "user".to_string(),
                content: json!(responder_prompt),
            }];

            let responder_system_prompt = system_prompt_for_thread
                .as_deref()
                .filter(|prompt| !prompt.trim().is_empty());

            let prepared_responder_messages = if provider == "anthropic" || provider == "claude_cli" {
                responder_messages.clone()
            } else {
                let mut prepared = responder_messages.clone();
                if let Some(system_prompt) = responder_system_prompt {
                    prepared.insert(
                        0,
                        LlmMessage {
                            role: "system".to_string(),
                            content: json!(system_prompt),
                        },
                    );
                }
                prepared
            };

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
            stream_started = true;

            let mut streamed_text = String::new();
            let mut on_chunk = |chunk: &str| {
                streamed_text.push_str(chunk);
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
            };

            let stream_result = match provider.as_str() {
                "openai" => {
                    let api_key = ModelOperations::get_api_key(&db, "openai")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    if api_key.is_empty() {
                        Err("Missing OpenAI API key".to_string())
                    } else {
                        stream_openai(
                            &client,
                            &api_key,
                            "https://api.openai.com/v1/chat/completions",
                            &model_for_thread,
                            &prepared_responder_messages,
                            &mut on_chunk,
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
                    } else {
                        stream_anthropic(
                            &client,
                            &api_key,
                            &model_for_thread,
                            responder_system_prompt,
                            &responder_messages,
                            &mut on_chunk,
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
                    } else {
                        stream_openai_compatible(
                            &client,
                            Some(&api_key),
                            "https://api.deepseek.com/chat/completions",
                            &model_for_thread,
                            &prepared_responder_messages,
                            false,
                            &mut on_chunk,
                        )
                    }
                }
                "custom" | "ollama" => {
                    let (url, api_key) = custom_backend_config.clone().unwrap_or_default();
                    if url.is_empty() {
                        Err("Missing custom backend URL".to_string())
                    } else {
                        stream_openai_compatible(
                            &client,
                            api_key.as_deref(),
                            &url,
                            &model_for_thread,
                            &prepared_responder_messages,
                            false,
                            &mut on_chunk,
                        )
                    }
                }
                _ => Err(format!("Unsupported provider: {provider}")),
            };

            let mut responder_usage: Option<Usage> = None;
            match stream_result {
                Ok(result) => {
                    if !result.content.trim().is_empty() {
                        final_response = result.content;
                    } else {
                        final_response = streamed_text;
                    }
                    responder_usage = result.usage;
                }
                Err(error) => {
                    log::error!(
                        "[agent] responder stream failed: provider={} model={} conversation_id={} message_id={} error={}",
                        provider,
                        model_for_thread,
                        conversation_id_for_thread,
                        assistant_message_id_for_thread,
                        error
                    );
                    final_response = draft.clone();
                }
            }

            if responder_usage.is_none() && !final_response.is_empty() {
                responder_usage = Some(Usage {
                    prompt_tokens: estimate_prompt_tokens(&prepared_responder_messages),
                    completion_tokens: estimate_tokens(&final_response),
                });
            }

            if let Some(usage) = responder_usage {
                usage_accumulator.prompt_tokens += usage.prompt_tokens;
                usage_accumulator.completion_tokens += usage.completion_tokens;
            }
        }

        if !stream_started {
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

            if !final_response.is_empty() {
                stream_response_chunks(
                    &bus,
                    &conversation_id_for_thread,
                    &assistant_message_id_for_thread,
                    &final_response,
                );
            }
        }

        if !final_response.is_empty() {
            let _ = MessageOperations::save_message(
                &db,
                &conversation_id_for_thread,
                "assistant",
                &final_response,
                &[],
                Some(assistant_message_id_for_thread.clone()),
            );

            for input in tool_execution_inputs {
                let _ = MessageOperations::save_tool_execution(&db, input);
            }

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
                    "content": final_response,
                    "attachments": [],
                    "timestamp_ms": timestamp_ms
                }),
                timestamp_ms,
            ));

            let note_db = db.clone();
            let note_provider = provider.clone();
            let note_model = model_for_thread.clone();
            let note_custom_backend = custom_backend_config.clone();
            let note_user_message = content.clone();
            let note_assistant_message = final_response.clone();
            let note_conversation_id = conversation_id_for_thread.clone();
            let note_assistant_message_id = assistant_message_id_for_thread.clone();
            std::thread::spawn(move || {
                if let Err(err) = capture_topic_note(
                    note_db,
                    note_provider,
                    note_model,
                    note_custom_backend,
                    note_user_message,
                    note_assistant_message,
                ) {
                    log::error!(
                        "[notes] capture failed: conversation_id={} message_id={} error={}",
                        note_conversation_id,
                        note_assistant_message_id,
                        err
                    );
                }
            });
        }

        let usage = if usage_accumulator.prompt_tokens > 0 || usage_accumulator.completion_tokens > 0 {
            Some(usage_accumulator)
        } else if final_response.is_empty() {
            None
        } else {
            Some(Usage {
                prompt_tokens: estimate_prompt_tokens(&messages_for_usage),
                completion_tokens: estimate_tokens(&final_response),
            })
        };

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
                "content": final_response,
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
        "claude_cli" => complete_claude_cli(&model, Some(system_prompt), &messages, None),
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

fn supports_streaming(provider: &str) -> bool {
    matches!(provider, "openai" | "anthropic" | "deepseek" | "custom" | "ollama")
}

fn build_responder_prompt(
    user_message: &str,
    messages: &[LlmMessage],
    tool_execution_inputs: &[MessageToolExecutionInput],
    draft: &str,
) -> String {
    let recent_messages = render_recent_messages(messages, 8);
    let tool_outputs = render_tool_outputs(tool_execution_inputs);
    let draft_text = if draft.trim().is_empty() { "None" } else { draft };
    let recent_text = if recent_messages.trim().is_empty() {
        "None"
    } else {
        recent_messages.as_str()
    };

    RESPONDER_PROMPT
        .replace("{user_message}", user_message)
        .replace("{recent_messages}", recent_text)
        .replace("{tool_outputs}", &tool_outputs)
        .replace("{draft}", draft_text)
}

fn render_recent_messages(messages: &[LlmMessage], limit: usize) -> String {
    let start = messages.len().saturating_sub(limit);
    messages
        .iter()
        .skip(start)
        .map(|message| format!("{}: {}", message.role, value_to_string(&message.content)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_tool_outputs(tool_execution_inputs: &[MessageToolExecutionInput]) -> String {
    if tool_execution_inputs.is_empty() {
        return "None".to_string();
    }

    let start = tool_execution_inputs.len().saturating_sub(6);
    let mut blocks = Vec::new();

    for input in tool_execution_inputs.iter().skip(start) {
        let parameters = serde_json::to_string_pretty(&input.parameters)
            .unwrap_or_else(|_| input.parameters.to_string());
        let result = serde_json::to_string_pretty(&input.result)
            .unwrap_or_else(|_| input.result.to_string());
        let error = input
            .error
            .as_ref()
            .map(|err| format!("\nError: {}", err))
            .unwrap_or_default();

        blocks.push(format!(
            "Tool: {}\nSuccess: {}\nArgs: {}\nResult: {}{}",
            input.tool_name, input.success, parameters, result, error
        ));
    }

    blocks.join("\n\n")
}
