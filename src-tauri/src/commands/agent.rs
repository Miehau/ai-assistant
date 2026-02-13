use crate::agent::prompts::RESPONDER_PROMPT;
use crate::agent::DynamicController;
use crate::db::{
    BranchOperations, ConversationOperations, CustomBackendOperations, Db, IncomingAttachment,
    MessageAttachment, MessageOperations, MessageToolExecution, MessageToolExecutionInput,
    ModelOperations, SaveMessageUsageInput, UsageOperations,
};
use crate::events::{
    AgentEvent, EventBus, EVENT_ASSISTANT_STREAM_CHUNK, EVENT_ASSISTANT_STREAM_COMPLETED,
    EVENT_ASSISTANT_STREAM_STARTED, EVENT_CONVERSATION_UPDATED, EVENT_MESSAGE_SAVED,
    EVENT_MESSAGE_USAGE_SAVED, EVENT_USAGE_UPDATED,
};
use crate::llm::{
    complete_anthropic, complete_anthropic_with_output_format_with_options, complete_claude_cli,
    complete_openai, complete_openai_compatible, complete_openai_compatible_with_options,
    complete_openai_with_options, stream_anthropic_with_options,
    stream_openai_compatible_with_options, stream_openai_with_options, LlmMessage,
    LlmRequestOptions, Usage,
};
use crate::tools::{ApprovalStore, ToolRegistry};
use chrono::Utc;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
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
static CANCEL_REGISTRY: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
const LLM_HTTP_TIMEOUT_SECS: u64 = 120;
const LLM_HTTP_CONNECT_TIMEOUT_SECS: u64 = 15;
const CONTROLLER_HTTP_TIMEOUT_SECS: u64 = 120;
const OPENAI_PROMPT_CACHE_RETENTION: &str = "24h";
const CACHE_DIAGNOSTICS_MIN_REQUESTS: u32 = 6;
const CACHE_DIAGNOSTICS_MIN_PROMPT_TOKENS: i64 = 4096;
const CACHE_DIAGNOSTICS_MIN_HIT_RATIO: f64 = 0.10;

fn get_pricing() -> &'static HashMap<String, PricingEntry> {
    PRICING.get_or_init(|| {
        let raw = include_str!("../../../src/lib/models/registry/pricing.json");
        let parsed: PricingData = serde_json::from_str(raw).unwrap_or(PricingData {
            pricing: HashMap::new(),
        });
        parsed.pricing
    })
}

fn cancel_registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    CANCEL_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_cancel_token(message_id: &str) -> Arc<AtomicBool> {
    let token = Arc::new(AtomicBool::new(false));
    let mut registry = cancel_registry().lock().unwrap();
    registry.insert(message_id.to_string(), token.clone());
    token
}

fn remove_cancel_token(message_id: &str) {
    let mut registry = cancel_registry().lock().unwrap();
    registry.remove(message_id);
}

fn cancel_token(message_id: &str) -> bool {
    let token = {
        let registry = cancel_registry().lock().unwrap();
        registry.get(message_id).cloned()
    };
    if let Some(token) = token {
        token.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

fn build_http_client_with_timeouts(timeout_secs: u64, connect_timeout_secs: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .build()
        .unwrap_or_else(|error| {
            log::warn!(
                "[agent] failed to build HTTP client with timeouts, falling back to defaults: {}",
                error
            );
            Client::new()
        })
}

fn build_http_client() -> Client {
    build_http_client_with_timeouts(LLM_HTTP_TIMEOUT_SECS, LLM_HTTP_CONNECT_TIMEOUT_SECS)
}

fn should_retry_anthropic_without_output_format(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    lowered.contains("timed out")
        || lowered.contains("deadline")
        || lowered.contains("error sending request for url")
        || lowered.contains("connection reset")
        || lowered.contains("stream error")
        || lowered.contains("http2")
}

fn controller_output_format_for_provider(provider: &str, output_format: Option<Value>) -> Option<Value> {
    if provider == "anthropic" {
        None
    } else {
        output_format
    }
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

fn supports_openai_prompt_cache_retention(model: &str) -> bool {
    let normalized = model.to_ascii_lowercase();
    normalized.starts_with("gpt-5")
}

fn llm_request_options(
    provider: &str,
    conversation_id: &str,
    phase: &str,
    model: &str,
) -> LlmRequestOptions {
    if provider == "openai" {
        let prompt_cache_key = format!("conversation:{conversation_id}:{phase}:v1");
        let prompt_cache_retention = if supports_openai_prompt_cache_retention(model) {
            Some(OPENAI_PROMPT_CACHE_RETENTION.to_string())
        } else {
            None
        };

        return LlmRequestOptions {
            prompt_cache_key: Some(prompt_cache_key),
            prompt_cache_retention,
            anthropic_cache_breakpoints: Vec::new(),
        };
    }

    if provider == "anthropic" {
        let anthropic_cache_breakpoints = if phase == "responder" {
            vec![0]
        } else {
            Vec::new()
        };
        return LlmRequestOptions {
            prompt_cache_key: None,
            prompt_cache_retention: None,
            anthropic_cache_breakpoints,
        };
    }

    LlmRequestOptions::default()
}

#[derive(Default)]
struct CacheDiagnostics {
    requests: u32,
    prompt_tokens: i64,
    cached_tokens: i64,
}

fn effective_prompt_tokens_for_cache(provider: &str, usage: &Usage) -> i64 {
    match provider {
        "anthropic" => (usage.prompt_tokens as i64
            + usage.cache_read_input_tokens.max(0) as i64
            + usage.cache_creation_input_tokens.max(0) as i64)
            .max(0),
        _ => (usage.prompt_tokens as i64).max(0),
    }
}

fn record_cache_diagnostics(
    provider: &str,
    model: &str,
    phase: &str,
    usage: &Usage,
    request_options: &LlmRequestOptions,
    diagnostics: &mut CacheDiagnostics,
) {
    let cached_tokens = match provider {
        "openai" => usage.cached_prompt_tokens as i64,
        "anthropic" => usage.cache_read_input_tokens as i64,
        _ => 0,
    };
    let request_prompt_tokens = effective_prompt_tokens_for_cache(provider, usage);
    if request_prompt_tokens <= 0 {
        return;
    }
    if !cache_diagnostics_enabled(provider, request_options) {
        return;
    }
    let request_cache_creation_tokens = match provider {
        "anthropic" => usage.cache_creation_input_tokens.max(0) as i64,
        _ => 0,
    };

    diagnostics.requests += 1;
    diagnostics.prompt_tokens += request_prompt_tokens;
    diagnostics.cached_tokens += cached_tokens.max(0);

    let request_cached_tokens = cached_tokens.max(0);
    let request_hit_ratio = if request_prompt_tokens > 0 {
        request_cached_tokens as f64 / request_prompt_tokens as f64
    } else {
        0.0
    };
    let cumulative_hit_ratio = if diagnostics.prompt_tokens > 0 {
        diagnostics.cached_tokens as f64 / diagnostics.prompt_tokens as f64
    } else {
        0.0
    };

    log::info!(
        "[cache] provider={} model={} phase={} cache_hit={} request_prompt_tokens={} request_cached_tokens={} request_cache_creation_tokens={} request_hit_ratio={:.3} cumulative_hit_ratio={:.3}",
        provider,
        model,
        phase,
        request_cached_tokens > 0,
        request_prompt_tokens,
        request_cached_tokens,
        request_cache_creation_tokens,
        request_hit_ratio,
        cumulative_hit_ratio
    );

    if diagnostics.requests >= CACHE_DIAGNOSTICS_MIN_REQUESTS
        && diagnostics.prompt_tokens >= CACHE_DIAGNOSTICS_MIN_PROMPT_TOKENS
        && cumulative_hit_ratio < CACHE_DIAGNOSTICS_MIN_HIT_RATIO
    {
        log::warn!(
            "[cache] low hit ratio: provider={} model={} phase={} hit_ratio={:.3} requests={} total_prompt_tokens={} total_cached_tokens={} prompt_cache_key={:?} anthropic_breakpoints={:?}",
            provider,
            model,
            phase,
            cumulative_hit_ratio,
            diagnostics.requests,
            diagnostics.prompt_tokens,
            diagnostics.cached_tokens,
            request_options.prompt_cache_key,
            request_options.anthropic_cache_breakpoints
        );
    }
}

fn cache_diagnostics_enabled(provider: &str, request_options: &LlmRequestOptions) -> bool {
    match provider {
        "openai" => request_options
            .prompt_cache_key
            .as_ref()
            .map(|key| !key.trim().is_empty())
            .unwrap_or(false),
        "anthropic" => !request_options.anthropic_cache_breakpoints.is_empty(),
        _ => false,
    }
}

fn stream_response_chunks(
    bus: &EventBus,
    conversation_id: &str,
    message_id: &str,
    content: &str,
    cancel_token: &Arc<AtomicBool>,
) {
    if content.is_empty() || cancel_token.load(Ordering::Relaxed) {
        return;
    }

    let chunk_size = if content.len() > 8000 { 400 } else { 240 };
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
        if cancel_token.load(Ordering::Relaxed) {
            return;
        }
        chunk.push(ch);
        if chunk.len() >= chunk_size {
            if cancel_token.load(Ordering::Relaxed) {
                return;
            }
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

    if !chunk.is_empty() && !cancel_token.load(Ordering::Relaxed) {
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

#[derive(Debug, Deserialize)]
pub struct AgentCancelPayload {
    pub message_id: String,
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
    log::info!(
        "[agent] send_message received: conversation_id={} provider={} model={} content_chars={} attachments={}",
        conversation_id,
        provider,
        model,
        content.chars().count(),
        attachments.len()
    );
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

    let assistant_message_id = assistant_message_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let history =
        MessageOperations::get_messages(&*state, &conversation_id).map_err(|e| e.to_string())?;

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
        let content = if message.role == "user" {
            let mapped_attachments = map_message_attachments(&message.attachments);
            if mapped_attachments.is_empty() {
                json!(message.content)
            } else {
                build_user_content(&message.content, &mapped_attachments)
            }
        } else {
            let mut content_text = message.content.clone();
            if !message.tool_executions.is_empty() {
                let tool_summary = format_tool_executions(&message.tool_executions);
                if !tool_summary.is_empty() {
                    content_text.push_str("\n\n");
                    content_text.push_str(&tool_summary);
                }
            }
            json!(content_text)
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
            let backend_id = custom_backend_id
                .clone()
                .ok_or_else(|| "Custom provider requires custom_backend_id".to_string())?;
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
    let cancel_token_for_thread = register_cancel_token(&assistant_message_id);

    std::thread::spawn(move || {
        log::info!(
            "[agent] worker started: conversation_id={} message_id={} provider={} model={}",
            conversation_id_for_thread,
            assistant_message_id_for_thread,
            provider,
            model_for_thread
        );
        let panic_bus = bus.clone();
        let panic_conversation_id = conversation_id_for_thread.clone();
        let panic_message_id = assistant_message_id_for_thread.clone();

        let worker_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let controller_client = build_http_client_with_timeouts(
                CONTROLLER_HTTP_TIMEOUT_SECS,
                LLM_HTTP_CONNECT_TIMEOUT_SECS,
            );
            let stream_client = build_http_client();
            let mut draft = String::new();
            let mut usage_accumulator = Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cached_prompt_tokens: 0,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            };
            let mut controller_cache_diagnostics = CacheDiagnostics::default();
            let mut responder_cache_diagnostics = CacheDiagnostics::default();
            let mut requested_user_input = false;
            let openai_api_key = ModelOperations::get_api_key(&db, "openai")
                .ok()
                .flatten()
                .unwrap_or_default();
            let anthropic_api_key = ModelOperations::get_api_key(&db, "anthropic")
                .ok()
                .flatten()
                .unwrap_or_default();
            let deepseek_api_key = ModelOperations::get_api_key(&db, "deepseek")
                .ok()
                .flatten()
                .unwrap_or_default();

            let custom_backend_config = if provider == "custom" {
                custom_backend_id
                    .as_ref()
                    .and_then(|id| CustomBackendOperations::get_custom_backend_by_id(&db, id).ok())
                    .flatten()
                    .map(|backend| (backend.url, backend.api_key))
            } else if provider == "ollama" {
                Some((
                    "http://localhost:11434/v1/chat/completions".to_string(),
                    None,
                ))
            } else {
                None
            };

            let messages_for_usage = messages.clone();
            let controller_request_options = llm_request_options(
                &provider,
                &conversation_id_for_thread,
                "controller",
                &model_for_thread,
            );
            let responder_request_options = llm_request_options(
                &provider,
                &conversation_id_for_thread,
                "responder",
                &model_for_thread,
            );

            let mut tool_execution_inputs: Vec<MessageToolExecutionInput> = Vec::new();
            let mut call_llm = |messages: &[LlmMessage],
                                system_prompt: Option<&str>,
                                output_format: Option<Value>| {
                log::debug!(
                    "[agent] controller llm call: provider={} model={} conversation_id={} message_id={} messages={} output_format={}",
                    provider,
                    model_for_thread,
                    conversation_id_for_thread,
                    assistant_message_id_for_thread,
                    messages.len(),
                    output_format.is_some()
                );
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

                let llm_call_started = Instant::now();
                let result = match provider.as_str() {
                    "openai" => {
                        if openai_api_key.is_empty() {
                            Err("Missing OpenAI API key".to_string())
                        } else {
                            complete_openai_with_options(
                                &controller_client,
                                &openai_api_key,
                                "https://api.openai.com/v1/chat/completions",
                                &model_for_thread,
                                &prepared_messages,
                                Some(&controller_request_options),
                            )
                        }
                    }
                    "anthropic" => {
                        if anthropic_api_key.is_empty() {
                            Err("Missing Anthropic API key".to_string())
                        } else {
                            let effective_output_format =
                                controller_output_format_for_provider(&provider, output_format.clone());
                            let primary = complete_anthropic_with_output_format_with_options(
                                &controller_client,
                                &anthropic_api_key,
                                &model_for_thread,
                                system_prompt,
                                &prepared_messages,
                                effective_output_format.clone(),
                                Some(&controller_request_options),
                            );
                            if effective_output_format.is_some() {
                                match primary {
                                    Ok(success) => Ok(success),
                                    Err(error)
                                        if should_retry_anthropic_without_output_format(&error) =>
                                    {
                                        log::warn!(
                                            "[agent] controller anthropic call failed with structured output, retrying without output_format and without anthropic cache options: conversation_id={} message_id={} error={}",
                                            conversation_id_for_thread,
                                            assistant_message_id_for_thread,
                                            error
                                        );
                                        complete_anthropic_with_output_format_with_options(
                                            &controller_client,
                                            &anthropic_api_key,
                                            &model_for_thread,
                                            system_prompt,
                                            &prepared_messages,
                                            None,
                                            None,
                                        )
                                        .map_err(|retry_error| {
                                            format!(
                                                "Anthropic controller retry without output_format failed: initial_error={error}; retry_error={retry_error}"
                                            )
                                        })
                                    }
                                    Err(error) => Err(error),
                                }
                            } else {
                                primary
                            }
                        }
                    }
                    "deepseek" => {
                        if deepseek_api_key.is_empty() {
                            Err("Missing DeepSeek API key".to_string())
                        } else {
                            complete_openai_compatible_with_options(
                                &controller_client,
                                Some(&deepseek_api_key),
                                "https://api.deepseek.com/chat/completions",
                                &model_for_thread,
                                &prepared_messages,
                                Some(&controller_request_options),
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
                            complete_openai_compatible_with_options(
                                &controller_client,
                                api_key.as_deref(),
                                &url,
                                &model_for_thread,
                                &prepared_messages,
                                Some(&controller_request_options),
                            )
                        }
                    }
                    _ => Err(format!("Unsupported provider: {provider}")),
                };

                let elapsed_ms = llm_call_started.elapsed().as_millis();
                match &result {
                    Ok(stream_result) => {
                        log::debug!(
                            "[agent] controller llm call completed: provider={} model={} conversation_id={} message_id={} elapsed_ms={} response_chars={}",
                            provider,
                            model_for_thread,
                            conversation_id_for_thread,
                            assistant_message_id_for_thread,
                            elapsed_ms,
                            stream_result.content.chars().count()
                        );
                    }
                    Err(error) => {
                        log::warn!(
                            "[agent] controller llm call failed: provider={} model={} conversation_id={} message_id={} elapsed_ms={} error={}",
                            provider,
                            model_for_thread,
                            conversation_id_for_thread,
                            assistant_message_id_for_thread,
                            elapsed_ms,
                            error
                        );
                    }
                }

                if let Ok(ref stream_result) = result {
                    if let Some(usage) = stream_result.usage.as_ref() {
                        usage_accumulator.prompt_tokens += usage.prompt_tokens;
                        usage_accumulator.completion_tokens += usage.completion_tokens;
                        usage_accumulator.cached_prompt_tokens += usage.cached_prompt_tokens;
                        usage_accumulator.cache_read_input_tokens += usage.cache_read_input_tokens;
                        usage_accumulator.cache_creation_input_tokens +=
                            usage.cache_creation_input_tokens;
                        record_cache_diagnostics(
                            &provider,
                            &model_for_thread,
                            "controller",
                            usage,
                            &controller_request_options,
                            &mut controller_cache_diagnostics,
                        );
                    } else {
                        usage_accumulator.prompt_tokens +=
                            estimate_prompt_tokens(&prepared_messages);
                        usage_accumulator.completion_tokens +=
                            estimate_tokens(&stream_result.content);
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
                cancel_token_for_thread.clone(),
                messages,
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
                log::info!(
                    "[agent] controller run started: conversation_id={} message_id={}",
                    conversation_id_for_thread,
                    assistant_message_id_for_thread
                );
                match controller.run(&content, &mut call_llm) {
                    Ok(response) => {
                        log::info!(
                            "[agent] controller run completed: conversation_id={} message_id={} response_chars={}",
                            conversation_id_for_thread,
                            assistant_message_id_for_thread,
                            response.chars().count()
                        );
                        draft = response;
                        controller_ok = true;
                        requested_user_input = controller.requested_user_input();
                    }
                    Err(error) => {
                        log::warn!(
                            "[agent] controller run failed: conversation_id={} message_id={} error={}",
                            conversation_id_for_thread,
                            assistant_message_id_for_thread,
                            error
                        );
                        if error == "Cancelled" {
                            draft.clear();
                        } else {
                            draft = format!("Agent error: {}", error);
                        }
                    }
                }
                tool_execution_inputs = controller.take_tool_executions();
            }

            let mut final_response = draft.clone();
            let mut stream_started = false;
            let mut cancelled = cancel_token_for_thread.load(Ordering::Relaxed);

            let stream_supported = supports_streaming(&provider);
            let use_responder = controller_ok
                && stream_supported
                && !tool_execution_inputs.is_empty()
                && !requested_user_input
                && !cancelled;

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

                let prepared_responder_messages =
                    if provider == "anthropic" || provider == "claude_cli" {
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

                if !cancel_token_for_thread.load(Ordering::Relaxed) {
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
                }

                let mut streamed_text = String::new();
                let cancel_token_for_chunks = cancel_token_for_thread.clone();
                let mut on_chunk = |chunk: &str| {
                    if cancel_token_for_chunks.load(Ordering::Relaxed) {
                        return;
                    }
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
                        if openai_api_key.is_empty() {
                            Err("Missing OpenAI API key".to_string())
                        } else {
                            stream_openai_with_options(
                                &stream_client,
                                &openai_api_key,
                                "https://api.openai.com/v1/chat/completions",
                                &model_for_thread,
                                &prepared_responder_messages,
                                Some(&responder_request_options),
                                &mut on_chunk,
                            )
                        }
                    }
                    "anthropic" => {
                        if anthropic_api_key.is_empty() {
                            Err("Missing Anthropic API key".to_string())
                        } else {
                            stream_anthropic_with_options(
                                &stream_client,
                                &anthropic_api_key,
                                &model_for_thread,
                                responder_system_prompt,
                                &responder_messages,
                                Some(&responder_request_options),
                                &mut on_chunk,
                            )
                        }
                    }
                    "deepseek" => {
                        if deepseek_api_key.is_empty() {
                            Err("Missing DeepSeek API key".to_string())
                        } else {
                            stream_openai_compatible_with_options(
                                &stream_client,
                                Some(&deepseek_api_key),
                                "https://api.deepseek.com/chat/completions",
                                &model_for_thread,
                                &prepared_responder_messages,
                                false,
                                Some(&responder_request_options),
                                &mut on_chunk,
                            )
                        }
                    }
                    "custom" | "ollama" => {
                        let (url, api_key) = custom_backend_config.clone().unwrap_or_default();
                        if url.is_empty() {
                            Err("Missing custom backend URL".to_string())
                        } else {
                            stream_openai_compatible_with_options(
                                &stream_client,
                                api_key.as_deref(),
                                &url,
                                &model_for_thread,
                                &prepared_responder_messages,
                                false,
                                Some(&responder_request_options),
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
                        cached_prompt_tokens: 0,
                        cache_read_input_tokens: 0,
                        cache_creation_input_tokens: 0,
                    });
                }

                if let Some(usage) = responder_usage {
                    usage_accumulator.prompt_tokens += usage.prompt_tokens;
                    usage_accumulator.completion_tokens += usage.completion_tokens;
                    usage_accumulator.cached_prompt_tokens += usage.cached_prompt_tokens;
                    usage_accumulator.cache_read_input_tokens += usage.cache_read_input_tokens;
                    usage_accumulator.cache_creation_input_tokens +=
                        usage.cache_creation_input_tokens;
                    record_cache_diagnostics(
                        &provider,
                        &model_for_thread,
                        "responder",
                        &usage,
                        &responder_request_options,
                        &mut responder_cache_diagnostics,
                    );
                }
                cancelled = cancel_token_for_thread.load(Ordering::Relaxed);
                if cancelled {
                    final_response.clear();
                    tool_execution_inputs.clear();
                }
            }

            if !stream_started && !cancelled {
                if !cancel_token_for_thread.load(Ordering::Relaxed) {
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
                }

                if !final_response.is_empty() {
                    stream_response_chunks(
                        &bus,
                        &conversation_id_for_thread,
                        &assistant_message_id_for_thread,
                        &final_response,
                        &cancel_token_for_thread,
                    );
                }
                cancelled = cancel_token_for_thread.load(Ordering::Relaxed);
                if cancelled {
                    final_response.clear();
                    tool_execution_inputs.clear();
                }
            }

            let should_persist_assistant_message = !cancelled
                && !cancel_token_for_thread.load(Ordering::Relaxed)
                && (!final_response.is_empty() || !tool_execution_inputs.is_empty());

            if should_persist_assistant_message {
                let _ = MessageOperations::save_message(
                    &db,
                    &conversation_id_for_thread,
                    "assistant",
                    &final_response,
                    &[],
                    Some(assistant_message_id_for_thread.clone()),
                );

                let tool_execution_payload: Vec<Value> = tool_execution_inputs
                    .iter()
                    .map(|input| {
                        json!({
                            "id": input.id,
                            "message_id": input.message_id,
                            "tool_name": input.tool_name,
                            "parameters": input.parameters,
                            "result": input.result,
                            "success": input.success,
                            "duration_ms": input.duration_ms,
                            "timestamp_ms": input.timestamp_ms,
                            "error": input.error,
                            "iteration_number": input.iteration_number
                        })
                    })
                    .collect();

                if !tool_execution_inputs.is_empty() {
                    for input in tool_execution_inputs {
                        let _ = MessageOperations::save_tool_execution(&db, input);
                    }
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
                        "tool_executions": tool_execution_payload,
                        "timestamp_ms": timestamp_ms
                    }),
                    timestamp_ms,
                ));
            }

            let usage =
                if usage_accumulator.prompt_tokens > 0 || usage_accumulator.completion_tokens > 0 {
                    Some(usage_accumulator)
                } else if final_response.is_empty() {
                    None
                } else {
                    Some(Usage {
                        prompt_tokens: estimate_prompt_tokens(&messages_for_usage),
                        completion_tokens: estimate_tokens(&final_response),
                        cached_prompt_tokens: 0,
                        cache_read_input_tokens: 0,
                        cache_creation_input_tokens: 0,
                    })
                };

            if let Some(usage) =
                usage.filter(|_| !cancelled && !cancel_token_for_thread.load(Ordering::Relaxed))
            {
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

                if let Ok(summary) =
                    UsageOperations::update_conversation_usage(&db, &conversation_id_for_thread)
                {
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
                    "content": if cancelled { String::new() } else { final_response },
                    "timestamp_ms": timestamp_ms
                }),
                timestamp_ms,
            ));

            remove_cancel_token(&assistant_message_id_for_thread);
        }));

        if worker_result.is_err() {
            log::error!(
                "[agent] worker panicked: conversation_id={} message_id={}",
                panic_conversation_id,
                panic_message_id
            );
            let timestamp_ms = Utc::now().timestamp_millis();
            panic_bus.publish(AgentEvent::new_with_timestamp(
                EVENT_ASSISTANT_STREAM_COMPLETED,
                json!({
                    "conversation_id": panic_conversation_id,
                    "message_id": panic_message_id,
                    "content": "Agent error: internal worker panic",
                    "timestamp_ms": timestamp_ms
                }),
                timestamp_ms,
            ));
            remove_cancel_token(&panic_message_id);
        }
    });

    Ok(AgentSendMessageResult {
        conversation_id,
        user_message_id,
        assistant_message_id,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn agent_cancel(payload: AgentCancelPayload) -> Result<(), String> {
    if cancel_token(&payload.message_id) {
        Ok(())
    } else {
        Err("No active agent request for message_id".to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn agent_generate_title(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    payload: AgentGenerateTitlePayload,
) -> Result<AgentGenerateTitleResult, String> {
    let db = state.inner().clone();
    let bus = event_bus.inner().clone();
    tauri::async_runtime::spawn_blocking(move || generate_title_and_update(db, bus, payload))
        .await
        .map_err(|err| format!("Title generation task failed: {err}"))?
}

fn generate_title_and_update(
    db: Db,
    event_bus: EventBus,
    payload: AgentGenerateTitlePayload,
) -> Result<AgentGenerateTitleResult, String> {
    log::info!(
        "[agent] title generation started: conversation_id={} provider={} model={}",
        payload.conversation_id,
        payload.provider,
        payload.model
    );
    let provider = payload.provider.to_lowercase();
    let model = payload.model.clone();

    let history = MessageOperations::get_messages(&db, &payload.conversation_id)
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

    let client = build_http_client();
    let mut title = match provider.as_str() {
        "openai" => {
            let api_key = ModelOperations::get_api_key(&db, "openai")
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
            let api_key = ModelOperations::get_api_key(&db, "anthropic")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if api_key.is_empty() {
                return Err("Missing Anthropic API key".to_string());
            }
            complete_anthropic(&client, &api_key, &model, Some(system_prompt), &messages)
        }
        "deepseek" => {
            let api_key = ModelOperations::get_api_key(&db, "deepseek")
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
                (
                    "http://localhost:11434/v1/chat/completions".to_string(),
                    None,
                )
            } else {
                let backend_id = payload
                    .custom_backend_id
                    .clone()
                    .ok_or_else(|| "Custom provider requires custom_backend_id".to_string())?;
                let backend = CustomBackendOperations::get_custom_backend_by_id(&db, &backend_id)
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

    title = title
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();
    if title.is_empty() {
        title = "New Conversation".to_string();
    }

    ConversationOperations::update_conversation_name(&db, &payload.conversation_id, &title)
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

const MAX_TOOL_ARGS_CHARS: usize = 900;
const MAX_TOOL_RESULT_CHARS: usize = 1200;
const MAX_TOOL_METADATA_CHARS: usize = 320;
const MAX_TOOL_ERROR_CHARS: usize = 2000;
const RESPONDER_HISTORY_MAX_CHARS: usize = 48_000;
const RESPONDER_HISTORY_STABLE_PREFIX_MESSAGES: usize = 8;
const RESPONDER_HISTORY_RECENT_TAIL_MESSAGES: usize = 20;

fn map_message_attachments(attachments: &[MessageAttachment]) -> Vec<IncomingAttachment> {
    attachments
        .iter()
        .map(|attachment| IncomingAttachment {
            name: attachment.name.clone(),
            data: attachment.data.clone(),
            attachment_type: attachment.attachment_type.clone(),
            description: attachment.description.clone(),
            transcript: attachment.transcript.clone(),
        })
        .collect()
}

fn truncate_for_prompt(value: &str, max_len: usize) -> String {
    let mut result = String::new();
    let mut count = 0usize;
    for ch in value.chars() {
        if count >= max_len {
            result.push_str(" ...(truncated)");
            return result;
        }
        result.push(ch);
        count += 1;
    }
    result
}

fn format_tool_executions(executions: &[MessageToolExecution]) -> String {
    if executions.is_empty() {
        return String::new();
    }

    let mut blocks = Vec::new();
    for exec in executions {
        blocks.push(format_tool_execution_block(
            &exec.tool_name,
            exec.success,
            &exec.parameters,
            &exec.result,
            exec.error.as_deref(),
            Some(exec.duration_ms),
            Some(exec.id.as_str()),
        ));
    }

    format!("[Tool executions]\n{}", blocks.join("\n\n"))
}

fn format_tool_execution_block(
    tool_name: &str,
    success: bool,
    parameters: &Value,
    result: &Value,
    error: Option<&str>,
    duration_ms: Option<i64>,
    execution_id: Option<&str>,
) -> String {
    let params = serde_json::to_string(parameters).unwrap_or_else(|_| parameters.to_string());
    let params = truncate_for_prompt(&params, MAX_TOOL_ARGS_CHARS);

    let output_ref = extract_output_ref_id(result).unwrap_or("none");
    let requested_output_mode = result
        .get("requested_output_mode")
        .and_then(|value| value.as_str())
        .unwrap_or("n/a");
    let resolved_output_mode = result
        .get("resolved_output_mode")
        .and_then(|value| value.as_str())
        .or_else(|| {
            if output_ref != "none" {
                Some("persist")
            } else if success {
                Some("inline")
            } else {
                None
            }
        })
        .unwrap_or("n/a");
    let forced_persist = result
        .get("forced_persist")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let forced_reason = result
        .get("forced_reason")
        .and_then(|value| value.as_str())
        .unwrap_or("none");

    let metadata = result
        .get("metadata")
        .cloned()
        .unwrap_or_else(|| compact_result_metadata(result));
    let metadata_summary = if metadata.is_null() {
        "none".to_string()
    } else {
        truncate_for_prompt(&metadata.to_string(), MAX_TOOL_METADATA_CHARS)
    };

    let preview = if !success {
        let failure = error.unwrap_or("Tool execution failed");
        format!(
            "Error: {}",
            truncate_for_prompt(failure, MAX_TOOL_ERROR_CHARS)
        )
    } else if let Some(preview) = result.get("preview").and_then(|value| value.as_str()) {
        let mut summary = truncate_for_prompt(preview, MAX_TOOL_RESULT_CHARS);
        if result
            .get("preview_truncated")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            summary.push_str(" ...(truncated)");
        }
        summary
    } else {
        let rendered = serde_json::to_string(result).unwrap_or_else(|_| result.to_string());
        truncate_for_prompt(&rendered, MAX_TOOL_RESULT_CHARS)
    };

    let mut prefix = format!("Tool: {tool_name}");
    if let Some(execution_id) = execution_id {
        prefix.push_str(&format!("\nExecutionId: {execution_id}"));
    }
    if let Some(duration_ms) = duration_ms {
        prefix.push_str(&format!("\nDurationMs: {duration_ms}"));
    }

    format!(
        "{prefix}\nSuccess: {success}\nRequestedOutputMode: {requested_output_mode}\nResolvedOutputMode: {resolved_output_mode}\nForcedPersist: {forced_persist}\nForcedReason: {forced_reason}\nOutputRef: {output_ref}\nArgs: {params}\nMetadata: {metadata_summary}\nPreview: {preview}"
    )
}

fn extract_output_ref_id(result: &Value) -> Option<&str> {
    result
        .get("output_ref")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
}

fn compact_result_metadata(value: &Value) -> Value {
    match value {
        Value::Object(map) => json!({
            "root_type": "object",
            "key_count": map.len()
        }),
        Value::Array(values) => json!({
            "root_type": "array",
            "array_length": values.len()
        }),
        Value::String(text) => json!({
            "root_type": "string",
            "string_length": text.chars().count()
        }),
        Value::Number(_) => json!({ "root_type": "number" }),
        Value::Bool(_) => json!({ "root_type": "boolean" }),
        Value::Null => json!({ "root_type": "null" }),
    }
}

fn supports_streaming(provider: &str) -> bool {
    matches!(
        provider,
        "openai" | "anthropic" | "deepseek" | "custom" | "ollama"
    )
}

fn build_responder_prompt(
    user_message: &str,
    messages: &[LlmMessage],
    tool_execution_inputs: &[MessageToolExecutionInput],
    draft: &str,
) -> String {
    let recent_messages = render_recent_messages(messages, messages.len());
    let tool_outputs = render_tool_outputs(tool_execution_inputs);
    let draft_text = if draft.trim().is_empty() {
        "None"
    } else {
        draft
    };
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
    let rendered = messages
        .iter()
        .skip(start)
        .map(|message| format!("{}: {}", message.role, value_to_string(&message.content)))
        .collect::<Vec<_>>();

    let total_chars = rendered
        .iter()
        .map(|entry| entry.chars().count())
        .sum::<usize>();
    if total_chars <= RESPONDER_HISTORY_MAX_CHARS {
        return rendered.join("\n");
    }

    let prefix_end = rendered.len().min(RESPONDER_HISTORY_STABLE_PREFIX_MESSAGES);
    let tail_start = rendered
        .len()
        .saturating_sub(RESPONDER_HISTORY_RECENT_TAIL_MESSAGES);
    if tail_start <= prefix_end {
        return rendered.join("\n");
    }

    let stable_prefix = rendered[..prefix_end].join("\n");
    let recent_tail = rendered[tail_start..].join("\n");
    let omitted_messages = tail_start - prefix_end;
    let omitted_chars = rendered[prefix_end..tail_start]
        .iter()
        .map(|entry| entry.chars().count())
        .sum::<usize>();

    format!(
        "{stable_prefix}\n[history_compact] omitted_middle_messages={omitted_messages} omitted_chars={omitted_chars}\n{recent_tail}"
    )
}

fn render_tool_outputs(tool_execution_inputs: &[MessageToolExecutionInput]) -> String {
    if tool_execution_inputs.is_empty() {
        return "None".to_string();
    }

    let start = tool_execution_inputs.len().saturating_sub(6);
    let mut blocks = Vec::new();

    for input in tool_execution_inputs.iter().skip(start) {
        blocks.push(format_tool_execution_block(
            &input.tool_name,
            input.success,
            &input.parameters,
            &input.result,
            input.error.as_deref(),
            Some(input.duration_ms),
            Some(input.id.as_str()),
        ));
    }

    blocks.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_request_options_disables_anthropic_cache_for_controller_phase() {
        let options = llm_request_options("anthropic", "conv-1", "controller", "claude-sonnet");
        assert!(options.anthropic_cache_breakpoints.is_empty());
    }

    #[test]
    fn llm_request_options_enables_anthropic_cache_for_responder_phase() {
        let options = llm_request_options("anthropic", "conv-1", "responder", "claude-sonnet");
        assert_eq!(options.anthropic_cache_breakpoints, vec![0]);
    }

    #[test]
    fn llm_request_options_disables_anthropic_cache_for_other_phases() {
        let options = llm_request_options("anthropic", "conv-1", "triage", "claude-sonnet");
        assert!(options.anthropic_cache_breakpoints.is_empty());
    }

    #[test]
    fn llm_request_options_keeps_openai_prompt_cache_key_shape() {
        let options = llm_request_options("openai", "conv-1", "controller", "gpt-5-mini");
        assert_eq!(
            options.prompt_cache_key.as_deref(),
            Some("conversation:conv-1:controller:v1")
        );
    }

    #[test]
    fn cache_diagnostics_enabled_for_openai_with_cache_key() {
        let options = llm_request_options("openai", "conv-1", "controller", "gpt-5-mini");
        assert!(cache_diagnostics_enabled("openai", &options));
    }

    #[test]
    fn cache_diagnostics_disabled_for_anthropic_controller_without_breakpoints() {
        let options = llm_request_options("anthropic", "conv-1", "controller", "claude-sonnet");
        assert!(!cache_diagnostics_enabled("anthropic", &options));
    }

    #[test]
    fn cache_diagnostics_enabled_for_anthropic_responder_with_breakpoints() {
        let options = llm_request_options("anthropic", "conv-1", "responder", "claude-sonnet");
        assert!(cache_diagnostics_enabled("anthropic", &options));
    }

    #[test]
    fn anthropic_retry_without_output_format_on_timeout_like_errors() {
        assert!(should_retry_anthropic_without_output_format(
            "error sending request for url: operation timed out"
        ));
        assert!(should_retry_anthropic_without_output_format(
            "error sending request for url (https://api.anthropic.com/v1/messages)"
        ));
        assert!(should_retry_anthropic_without_output_format(
            "error: stream error in the HTTP/2 framing layer"
        ));
    }

    #[test]
    fn anthropic_retry_without_output_format_skips_rate_limit_errors() {
        assert!(!should_retry_anthropic_without_output_format(
            "Anthropic error: 429 Too Many Requests - rate limit exceeded"
        ));
        assert!(!should_retry_anthropic_without_output_format(
            "Anthropic error: 400 Bad Request - invalid output format"
        ));
    }

    #[test]
    fn controller_output_format_for_provider_disables_anthropic_structured_output() {
        let format = json!({
            "type": "json_schema",
            "schema": { "type": "object" }
        });
        assert!(
            controller_output_format_for_provider("anthropic", Some(format)).is_none()
        );
    }

    #[test]
    fn controller_output_format_for_provider_keeps_non_anthropic_output_format() {
        let format = json!({
            "type": "json_schema",
            "schema": { "type": "object" }
        });
        let selected = controller_output_format_for_provider("openai", Some(format.clone()));
        assert_eq!(selected, Some(format));
    }

    #[test]
    fn format_tool_executions_renders_envelope_fields() {
        let execution = MessageToolExecution {
            id: "exec-1".to_string(),
            message_id: "msg-1".to_string(),
            tool_name: "gmail.list_threads".to_string(),
            parameters: serde_json::json!({ "max_results": 10 }),
            result: serde_json::json!({
                "persisted": true,
                "output_ref": { "id": "artifact-123" },
                "preview": "[{\"id\":\"t1\"}]",
                "preview_truncated": false,
                "metadata": { "root_type": "array", "array_length": 10 },
                "requested_output_mode": "persist",
                "resolved_output_mode": "persist",
                "forced_persist": false,
                "forced_reason": null
            }),
            success: true,
            duration_ms: 42,
            timestamp_ms: 1000,
            error: None,
            iteration_number: 1,
        };

        let rendered = format_tool_executions(&[execution]);
        assert!(rendered.contains("RequestedOutputMode: persist"));
        assert!(rendered.contains("ResolvedOutputMode: persist"));
        assert!(rendered.contains("OutputRef: artifact-123"));
        assert!(rendered.contains("Metadata:"));
        assert!(rendered.contains("Preview:"));
    }

    #[test]
    fn format_tool_executions_truncates_large_inline_result() {
        let big_payload = "x".repeat(MAX_TOOL_RESULT_CHARS * 3);
        let execution = MessageToolExecution {
            id: "exec-2".to_string(),
            message_id: "msg-2".to_string(),
            tool_name: "files.read".to_string(),
            parameters: serde_json::json!({ "path": "/tmp/file.txt" }),
            result: serde_json::json!({ "content": big_payload }),
            success: true,
            duration_ms: 7,
            timestamp_ms: 2000,
            error: None,
            iteration_number: 1,
        };

        let rendered = format_tool_executions(&[execution]);
        assert!(rendered.contains("ResolvedOutputMode: inline"));
        assert!(rendered.contains("...(truncated)"));
        assert!(
            rendered.len() < (MAX_TOOL_RESULT_CHARS * 2),
            "expected compact envelope rendering, got {} chars",
            rendered.len()
        );
    }
}
