use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::process::Command;

const PROVIDER_ERROR_BODY_MAX_CHARS: usize = 2_000;
const ANTHROPIC_CACHE_BLOCK_MAX_CHARS: usize = 2_500;
const ANTHROPIC_CACHE_INTERVAL_BLOCKS: usize = 16;

#[derive(Clone, Debug)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub cached_prompt_tokens: i32,
    pub cache_read_input_tokens: i32,
    pub cache_creation_input_tokens: i32,
}

pub struct StreamResult {
    pub content: String,
    pub usage: Option<Usage>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: Value,
}

#[derive(Clone, Debug, Default)]
pub struct LlmRequestOptions {
    pub prompt_cache_key: Option<String>,
    pub prompt_cache_retention: Option<String>,
    pub anthropic_cache_breakpoints: Vec<usize>,
}

fn compact_error_body(body: String) -> String {
    let normalized = body.trim().replace('\n', " ");
    if normalized.chars().count() <= PROVIDER_ERROR_BODY_MAX_CHARS {
        return normalized;
    }

    let truncated: String = normalized
        .chars()
        .take(PROVIDER_ERROR_BODY_MAX_CHARS)
        .collect();
    format!("{truncated}... [truncated]")
}

fn build_openai_compatible_body(
    model: &str,
    messages: &[LlmMessage],
    stream: bool,
    include_usage: bool,
    request_options: Option<&LlmRequestOptions>,
) -> Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": stream
    });

    if include_usage {
        body["stream_options"] = serde_json::json!({
            "include_usage": true
        });
    }

    if let Some(options) = request_options {
        if let Some(key) = options.prompt_cache_key.as_ref() {
            body["prompt_cache_key"] = serde_json::json!(key);
        }
        if let Some(retention) = options.prompt_cache_retention.as_ref() {
            body["prompt_cache_retention"] = serde_json::json!(retention);
        }
    }

    body
}

fn parse_openai_usage(usage: &Value) -> Option<Usage> {
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let completion_tokens = usage
        .get("completion_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let cached_prompt_tokens = usage
        .get("prompt_tokens_details")
        .and_then(|details| details.get("cached_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    if prompt_tokens > 0 || completion_tokens > 0 || cached_prompt_tokens > 0 {
        Some(Usage {
            prompt_tokens,
            completion_tokens,
            cached_prompt_tokens,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        })
    } else {
        None
    }
}

fn parse_anthropic_usage(usage: &Value) -> Option<Usage> {
    let prompt_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let completion_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let cache_read_input_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let cache_creation_input_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    if prompt_tokens > 0
        || completion_tokens > 0
        || cache_read_input_tokens > 0
        || cache_creation_input_tokens > 0
    {
        Some(Usage {
            prompt_tokens,
            completion_tokens,
            cached_prompt_tokens: 0,
            cache_read_input_tokens,
            cache_creation_input_tokens,
        })
    } else {
        None
    }
}

fn strip_anthropic_unsupported_schema_keywords(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("if");
            map.remove("then");
            map.remove("else");
            map.remove("allOf");
            map.remove("dependentSchemas");
            map.remove("unevaluatedProperties");

            let is_object_schema = map
                .get("type")
                .and_then(|value| value.as_str())
                .map(|kind| kind == "object")
                .unwrap_or(false)
                || map.contains_key("properties");
            if is_object_schema {
                map.insert("additionalProperties".to_string(), Value::Bool(false));
            }

            let is_numeric_schema = map
                .get("type")
                .and_then(|value| value.as_str())
                .map(|kind| kind == "number" || kind == "integer")
                .unwrap_or(false);
            if is_numeric_schema {
                map.remove("minimum");
                map.remove("maximum");
                map.remove("exclusiveMinimum");
                map.remove("exclusiveMaximum");
                map.remove("multipleOf");
            }

            for entry in map.values_mut() {
                strip_anthropic_unsupported_schema_keywords(entry);
            }
        }
        Value::Array(array) => {
            for entry in array {
                strip_anthropic_unsupported_schema_keywords(entry);
            }
        }
        _ => {}
    }
}

fn build_openai_output_schema(output_format: Option<Value>) -> Option<Value> {
    output_format
}

fn build_anthropic_output_schema(output_format: Option<Value>) -> Option<Value> {
    let mut output = output_format?;
    if let Some(schema) = output.get_mut("schema") {
        strip_anthropic_unsupported_schema_keywords(schema);
    } else {
        strip_anthropic_unsupported_schema_keywords(&mut output);
    }
    Some(output)
}

pub fn complete_openai(
    client: &Client,
    api_key: &str,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
) -> Result<StreamResult, String> {
    complete_openai_with_options(client, api_key, url, model, messages, None)
}

pub fn complete_openai_with_options(
    client: &Client,
    api_key: &str,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    request_options: Option<&LlmRequestOptions>,
) -> Result<StreamResult, String> {
    complete_openai_compatible_with_output_format_with_options(
        client,
        Some(api_key),
        url,
        model,
        messages,
        None,
        request_options,
    )
}

pub fn complete_openai_compatible(
    client: &Client,
    api_key: Option<&str>,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
) -> Result<StreamResult, String> {
    complete_openai_compatible_with_options(client, api_key, url, model, messages, None)
}

pub fn complete_openai_compatible_with_options(
    client: &Client,
    api_key: Option<&str>,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    request_options: Option<&LlmRequestOptions>,
) -> Result<StreamResult, String> {
    complete_openai_compatible_with_output_format_with_options(
        client,
        api_key,
        url,
        model,
        messages,
        None,
        request_options,
    )
}

pub fn complete_openai_compatible_with_output_format_with_options(
    client: &Client,
    api_key: Option<&str>,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    output_format: Option<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<StreamResult, String> {
    let mut body = build_openai_compatible_body(model, messages, false, false, request_options);
    if let Some(openai_output_schema) = build_openai_output_schema(output_format) {
        body["response_format"] = openai_output_schema;
    }

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body);

    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request.send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = compact_error_body(response.text().unwrap_or_default());
        return Err(format!("Provider error: {status} - {body}"));
    }

    let value: Value = response.json().map_err(|e| e.to_string())?;
    log::debug!(
        "[llm] provider=openai_compatible model={} raw_response={}",
        model,
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
    );

    let content = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_str())
        })
        .unwrap_or("")
        .to_string();

    let usage = value.get("usage").and_then(parse_openai_usage);

    log::debug!(
        "[llm] provider=openai_compatible model={} content_len={} usage={:?}",
        model,
        content.len(),
        usage
            .as_ref()
            .map(|u| {
                (
                    u.prompt_tokens,
                    u.completion_tokens,
                    u.cached_prompt_tokens,
                    u.cache_read_input_tokens,
                    u.cache_creation_input_tokens,
                )
            })
    );

    Ok(StreamResult { content, usage })
}

pub fn stream_openai_with_options<F>(
    client: &Client,
    api_key: &str,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    request_options: Option<&LlmRequestOptions>,
    on_chunk: &mut F,
) -> Result<StreamResult, String>
where
    F: FnMut(&str),
{
    stream_openai_compatible_with_options(
        client,
        Some(api_key),
        url,
        model,
        messages,
        true,
        request_options,
        on_chunk,
    )
}

pub fn stream_openai_compatible_with_options<F>(
    client: &Client,
    api_key: Option<&str>,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    include_usage: bool,
    request_options: Option<&LlmRequestOptions>,
    on_chunk: &mut F,
) -> Result<StreamResult, String>
where
    F: FnMut(&str),
{
    let body = build_openai_compatible_body(model, messages, true, include_usage, request_options);

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body);

    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request.send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = compact_error_body(response.text().unwrap_or_default());
        return Err(format!("Provider error: {status} - {body}"));
    }

    let mut reader = BufReader::new(response);
    let mut line = String::new();
    let mut content = String::new();
    let mut usage: Option<Usage> = None;

    while reader.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        if !trimmed.starts_with("data:") {
            line.clear();
            continue;
        }

        let data = trimmed.trim_start_matches("data:").trim();
        if data == "[DONE]" {
            break;
        }

        let value: Value = match serde_json::from_str(data) {
            Ok(value) => value,
            Err(_) => {
                line.clear();
                continue;
            }
        };

        if let Some(delta) = value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
        {
            let chunk = delta
                .get("content")
                .and_then(|v| v.as_str())
                .or_else(|| delta.get("text").and_then(|v| v.as_str()));
            if let Some(text) = chunk {
                content.push_str(text);
                on_chunk(text);
            }
        }

        if let Some(usage_value) = value.get("usage") {
            usage = parse_openai_usage(usage_value);
        }

        line.clear();
    }

    Ok(StreamResult { content, usage })
}

pub fn complete_anthropic(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
) -> Result<StreamResult, String> {
    complete_anthropic_with_options(client, api_key, model, system, messages, None)
}

fn chunk_text_by_chars(input: &str, max_chars: usize) -> Vec<String> {
    if input.is_empty() || max_chars == 0 {
        return Vec::new();
    }

    let chars: Vec<char> = input.chars().collect();
    chars
        .chunks(max_chars)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

fn split_anthropic_text_for_cache(input: &str) -> Vec<String> {
    let marker = ["\nSTATE SUMMARY:\n", "\nLAST TOOL OUTPUT:\n", "\nLIMITS:\n"]
        .iter()
        .filter_map(|candidate| input.find(candidate).map(|idx| (idx, *candidate)))
        .min_by_key(|(idx, _)| *idx);

    if let Some((idx, _)) = marker {
        let stable_prefix = &input[..idx];
        let dynamic_suffix = &input[idx..];
        let mut blocks = chunk_text_by_chars(stable_prefix, ANTHROPIC_CACHE_BLOCK_MAX_CHARS);
        blocks.extend(chunk_text_by_chars(
            dynamic_suffix,
            ANTHROPIC_CACHE_BLOCK_MAX_CHARS,
        ));
        return blocks;
    }

    chunk_text_by_chars(input, ANTHROPIC_CACHE_BLOCK_MAX_CHARS)
}

fn should_apply_anthropic_cache_control(
    block_index: usize,
    explicit_breakpoints: &[usize],
    cache_enabled: bool,
) -> bool {
    if !cache_enabled {
        return false;
    }

    explicit_breakpoints.contains(&block_index)
        || (block_index > 0 && block_index % ANTHROPIC_CACHE_INTERVAL_BLOCKS == 0)
}

fn format_anthropic_system(
    system: Option<&str>,
    block_index: &mut usize,
    request_options: Option<&LlmRequestOptions>,
) -> Option<Value> {
    let text = system?;
    if text.trim().is_empty() {
        return None;
    }

    let (explicit_breakpoints, cache_enabled) = request_options
        .map(|options| {
            (
                options.anthropic_cache_breakpoints.as_slice(),
                !options.anthropic_cache_breakpoints.is_empty(),
            )
        })
        .unwrap_or((&[] as &[usize], false));

    let mut content_blocks = Vec::new();
    for chunk in chunk_text_by_chars(text, ANTHROPIC_CACHE_BLOCK_MAX_CHARS) {
        if chunk.is_empty() {
            continue;
        }
        let mut block = serde_json::json!({
            "type": "text",
            "text": chunk
        });
        if should_apply_anthropic_cache_control(*block_index, explicit_breakpoints, cache_enabled) {
            block["cache_control"] = serde_json::json!({ "type": "ephemeral" });
        }
        content_blocks.push(block);
        *block_index += 1;
    }

    if content_blocks.is_empty() {
        None
    } else {
        Some(Value::Array(content_blocks))
    }
}

fn format_anthropic_messages(
    messages: &[LlmMessage],
    block_index: &mut usize,
    request_options: Option<&LlmRequestOptions>,
) -> Vec<Value> {
    let (explicit_breakpoints, cache_enabled) = request_options
        .map(|options| {
            (
                options.anthropic_cache_breakpoints.as_slice(),
                !options.anthropic_cache_breakpoints.is_empty(),
            )
        })
        .unwrap_or((&[] as &[usize], false));

    let mut formatted = Vec::new();
    for message in messages.iter().filter(|m| m.role != "system") {
        let text = value_to_string(&message.content);
        let chunks = split_anthropic_text_for_cache(&text);
        let mut content_blocks = Vec::new();

        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            let mut block = serde_json::json!({
                "type": "text",
                "text": chunk
            });
            if should_apply_anthropic_cache_control(
                *block_index,
                explicit_breakpoints,
                cache_enabled,
            ) {
                block["cache_control"] = serde_json::json!({ "type": "ephemeral" });
            }
            content_blocks.push(block);
            *block_index += 1;
        }

        if content_blocks.is_empty() {
            content_blocks.push(serde_json::json!({
                "type": "text",
                "text": ""
            }));
            *block_index += 1;
        }

        formatted.push(serde_json::json!({
            "role": message.role,
            "content": content_blocks
        }));
    }

    formatted
}

pub fn complete_anthropic_with_options(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    request_options: Option<&LlmRequestOptions>,
) -> Result<StreamResult, String> {
    complete_anthropic_with_output_format_with_options(
        client,
        api_key,
        model,
        system,
        messages,
        None,
        request_options,
    )
}

pub fn json_schema_output_format(schema: Value) -> Value {
    serde_json::json!({
        "type": "json_schema",
        "schema": schema
    })
}

pub fn complete_anthropic_with_output_format_with_options(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    output_format: Option<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<StreamResult, String> {
    let mut block_index = 0usize;
    let formatted_system = format_anthropic_system(system, &mut block_index, request_options);
    let formatted_messages = format_anthropic_messages(messages, &mut block_index, request_options);

    let mut body = serde_json::json!({
        "model": model,
        "messages": formatted_messages,
        "stream": false,
        "max_tokens": 4096,
        "temperature": 0,
    });

    if let Some(system_blocks) = formatted_system {
        body["system"] = system_blocks;
    }

    let sanitized_output_format = build_anthropic_output_schema(output_format);
    let has_output_format = sanitized_output_format.is_some();
    if let Some(output_format_value) = sanitized_output_format {
        body["output_format"] = output_format_value;
    }

    let send_request = |payload: &Value, structured_outputs: bool| -> Result<Value, String> {
        let mut request = client
            .post("https://api.anthropic.com/v1/messages")
            .header("Content-Type", "application/json")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");

        if structured_outputs {
            request = request.header("anthropic-beta", "structured-outputs-2025-11-13");
        }

        let response = request.json(payload).send().map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            let status = response.status();
            let body = compact_error_body(response.text().unwrap_or_default());
            return Err(format!("Anthropic error: {status} - {body}"));
        }

        response.json().map_err(|e| e.to_string())
    };

    let value = send_request(&body, has_output_format)?;

    log::debug!(
        "[llm] provider=anthropic model={} raw_response={}",
        model,
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
    );

    let content = value
        .get("content")
        .and_then(|content| content.as_array())
        .and_then(|arr| arr.first())
        .and_then(|block| block.get("text"))
        .and_then(|text| text.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            "Anthropic structured output missing expected content[0].text".to_string()
        })?;

    let usage = value.get("usage").and_then(parse_anthropic_usage);

    log::debug!(
        "[llm] provider=anthropic model={} content_len={} usage={:?}",
        model,
        content.len(),
        usage
            .as_ref()
            .map(|u| {
                (
                    u.prompt_tokens,
                    u.completion_tokens,
                    u.cached_prompt_tokens,
                    u.cache_read_input_tokens,
                    u.cache_creation_input_tokens,
                )
            })
    );

    Ok(StreamResult { content, usage })
}

pub fn stream_anthropic_with_options<F>(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    request_options: Option<&LlmRequestOptions>,
    on_chunk: &mut F,
) -> Result<StreamResult, String>
where
    F: FnMut(&str),
{
    let mut block_index = 0usize;
    let formatted_system = format_anthropic_system(system, &mut block_index, request_options);
    let formatted_messages = format_anthropic_messages(messages, &mut block_index, request_options);

    let mut body = serde_json::json!({
        "model": model,
        "messages": formatted_messages,
        "stream": true,
        "max_tokens": 4096,
        "temperature": 0,
    });

    if let Some(system_blocks) = formatted_system {
        body["system"] = system_blocks;
    }

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = compact_error_body(response.text().unwrap_or_default());
        return Err(format!("Anthropic error: {status} - {body}"));
    }

    let mut reader = BufReader::new(response);
    let mut line = String::new();
    let mut content = String::new();
    let mut usage: Option<Usage> = None;

    while reader.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        if !trimmed.starts_with("data:") {
            line.clear();
            continue;
        }

        let data = trimmed.trim_start_matches("data:").trim();
        if data == "[DONE]" {
            break;
        }

        let value: Value = match serde_json::from_str(data) {
            Ok(value) => value,
            Err(_) => {
                line.clear();
                continue;
            }
        };

        if let Some(event_type) = value.get("type").and_then(|v| v.as_str()) {
            if event_type == "content_block_delta" {
                if let Some(delta) = value.get("delta") {
                    let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if delta_type == "text_delta" {
                        if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                            content.push_str(text);
                            on_chunk(text);
                        }
                    }
                }
            }

            if event_type == "message_start" {
                if let Some(message) = value.get("message") {
                    if let Some(usage_value) = message.get("usage") {
                        usage = parse_anthropic_usage(usage_value);
                    }
                }
            }

            if event_type == "message_delta" {
                if let Some(usage_value) = value.get("usage") {
                    let parsed = parse_anthropic_usage(usage_value);
                    let previous = usage.clone().unwrap_or(Usage {
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        cached_prompt_tokens: 0,
                        cache_read_input_tokens: 0,
                        cache_creation_input_tokens: 0,
                    });
                    let completion_tokens = usage_value
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(previous.completion_tokens as i64)
                        as i32;
                    let prompt_tokens = parsed
                        .as_ref()
                        .map(|u| {
                            if u.prompt_tokens > 0 {
                                u.prompt_tokens
                            } else {
                                previous.prompt_tokens
                            }
                        })
                        .unwrap_or(previous.prompt_tokens);
                    let cache_read_input_tokens = parsed
                        .as_ref()
                        .map(|u| u.cache_read_input_tokens)
                        .unwrap_or(previous.cache_read_input_tokens);
                    let cache_creation_input_tokens = parsed
                        .as_ref()
                        .map(|u| u.cache_creation_input_tokens)
                        .unwrap_or(previous.cache_creation_input_tokens);
                    if prompt_tokens > 0
                        || completion_tokens > 0
                        || cache_read_input_tokens > 0
                        || cache_creation_input_tokens > 0
                    {
                        usage = Some(Usage {
                            prompt_tokens,
                            completion_tokens,
                            cached_prompt_tokens: 0,
                            cache_read_input_tokens,
                            cache_creation_input_tokens,
                        });
                    }
                }
            }
        }

        line.clear();
    }

    Ok(StreamResult { content, usage })
}

fn normalize_claude_cli_model(model: &str) -> String {
    if let Some(rest) = model.strip_prefix("claude-cli-") {
        return format!("claude-{}", rest);
    }
    model.to_string()
}

fn format_claude_cli_prompt(
    messages: &[LlmMessage],
    system: Option<&str>,
    output_format: Option<&Value>,
) -> String {
    let mut prompt = String::new();

    if let Some(format) = output_format {
        let schema = format.get("schema").unwrap_or(format);
        let schema_text =
            serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());
        prompt.push_str("Return ONLY valid JSON. No markdown, no extra text.\n");
        prompt.push_str("The JSON must conform to this schema:\n");
        prompt.push_str(&schema_text);
        prompt.push_str("\n");
        prompt.push_str("If action is \"complete\" or step.type is \"respond\", include a \"message\" field.\n\n");
    }

    if let Some(system_prompt) = system {
        let trimmed = system_prompt.trim();
        if !trimmed.is_empty() {
            prompt.push_str("System:\n");
            prompt.push_str(trimmed);
            prompt.push_str("\n\n");
        }
    }

    for message in messages.iter().filter(|m| m.role != "system") {
        let role_label = match message.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "tool" => "Tool",
            other => other,
        };
        prompt.push_str(role_label);
        prompt.push_str(":\n");
        prompt.push_str(value_to_string(&message.content).trim());
        prompt.push_str("\n\n");
    }

    prompt.push_str("Assistant:\n");
    prompt
}

pub fn complete_claude_cli(
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    output_format: Option<Value>,
) -> Result<StreamResult, String> {
    let prompt = format_claude_cli_prompt(messages, system, output_format.as_ref());
    let normalized_model = normalize_claude_cli_model(model);

    let mut command = Command::new("claude");
    command.arg("-p");
    command.arg(&prompt);
    if !normalized_model.trim().is_empty() {
        command.arg("--model").arg(&normalized_model);
    }

    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        return Err(format!("Claude CLI error: {}", message));
    }

    let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(StreamResult {
        content,
        usage: None,
    })
}

fn value_to_string(value: &Value) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn openai_body_includes_prompt_cache_fields() {
        let options = LlmRequestOptions {
            prompt_cache_key: Some("conversation:test:controller:v1".to_string()),
            prompt_cache_retention: Some("24h".to_string()),
            anthropic_cache_breakpoints: Vec::new(),
        };
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let body = build_openai_compatible_body("gpt-5-mini", &messages, true, true, Some(&options));

        assert_eq!(
            body.get("prompt_cache_key").and_then(|v| v.as_str()),
            Some("conversation:test:controller:v1")
        );
        assert_eq!(
            body.get("prompt_cache_retention").and_then(|v| v.as_str()),
            Some("24h")
        );
        assert_eq!(
            body.get("stream_options")
                .and_then(|v| v.get("include_usage"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn parse_openai_usage_extracts_cached_tokens() {
        let usage = parse_openai_usage(&json!({
            "prompt_tokens": 1000,
            "completion_tokens": 50,
            "prompt_tokens_details": {
                "cached_tokens": 700
            }
        }))
        .expect("expected usage");
        assert_eq!(usage.prompt_tokens, 1000);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.cached_prompt_tokens, 700);
    }

    #[test]
    fn parse_anthropic_usage_extracts_cache_fields() {
        let usage = parse_anthropic_usage(&json!({
            "input_tokens": 1200,
            "output_tokens": 75,
            "cache_read_input_tokens": 1000,
            "cache_creation_input_tokens": 200
        }))
        .expect("expected usage");
        assert_eq!(usage.prompt_tokens, 1200);
        assert_eq!(usage.completion_tokens, 75);
        assert_eq!(usage.cache_read_input_tokens, 1000);
        assert_eq!(usage.cache_creation_input_tokens, 200);
    }

    #[test]
    fn anthropic_format_marks_cache_breakpoints() {
        let options = LlmRequestOptions {
            prompt_cache_key: None,
            prompt_cache_retention: None,
            anthropic_cache_breakpoints: vec![0],
        };
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("stable prefix\nSTATE SUMMARY:\ndynamic suffix"),
        }];
        let mut block_index = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut block_index, Some(&options));
        let first_block = &formatted[0]["content"][0];
        assert_eq!(
            first_block
                .get("cache_control")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("ephemeral")
        );
    }

    #[test]
    fn anthropic_format_adds_periodic_cache_breakpoints() {
        let options = LlmRequestOptions {
            prompt_cache_key: None,
            prompt_cache_retention: None,
            anthropic_cache_breakpoints: vec![0],
        };
        let message = "a".repeat(
            ANTHROPIC_CACHE_BLOCK_MAX_CHARS * (ANTHROPIC_CACHE_INTERVAL_BLOCKS + 2),
        );
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!(message),
        }];
        let mut block_index = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut block_index, Some(&options));
        let blocks = formatted[0]["content"].as_array().expect("content array");

        assert_eq!(
            blocks[ANTHROPIC_CACHE_INTERVAL_BLOCKS]
                .get("cache_control")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("ephemeral")
        );
    }

    #[test]
    fn complete_openai_compatible_parses_cache_metrics_from_mock_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request_buffer = [0u8; 4096];
            let _ = stream.read(&mut request_buffer);

            let response_body = json!({
                "choices": [{
                    "message": { "content": "ok" }
                }],
                "usage": {
                    "prompt_tokens": 1000,
                    "completion_tokens": 25,
                    "prompt_tokens_details": {
                        "cached_tokens": 800
                    }
                }
            })
            .to_string();

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        let client = Client::builder().build().expect("client");
        let url = format!("http://{}/v1/chat/completions", addr);
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let result = complete_openai_compatible_with_options(
            &client,
            None,
            &url,
            "gpt-5-mini",
            &messages,
            None,
        )
        .expect("completion");
        let usage = result.usage.expect("usage");
        assert_eq!(usage.prompt_tokens, 1000);
        assert_eq!(usage.completion_tokens, 25);
        assert_eq!(usage.cached_prompt_tokens, 800);

        handle.join().expect("join server");
    }

    #[test]
    fn openai_schema_builder_is_passthrough() {
        let source = json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "allOf": [
                    {
                        "if": { "properties": { "action": { "const": "next_step" } } },
                        "then": { "required": ["step", "thinking"] }
                    }
                ]
            }
        });
        let built = build_openai_output_schema(Some(source.clone())).expect("output");
        assert_eq!(built, source);
    }

    #[test]
    fn anthropic_schema_builder_removes_if_then_allof() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string" }
                },
                "allOf": [
                    {
                        "if": { "properties": { "action": { "const": "next_step" } } },
                        "then": { "required": ["step", "thinking"] }
                    }
                ]
            }
        })))
        .expect("sanitized output");

        let schema = output.get("schema").expect("schema");
        assert!(schema.get("allOf").is_none());
        assert!(schema.get("if").is_none());
        assert!(schema.get("then").is_none());
        assert_eq!(
            schema
                .get("additionalProperties")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn anthropic_schema_builder_adds_additional_properties_false_recursively() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "properties": {
                    "step": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" }
                        }
                    }
                }
            }
        })))
        .expect("sanitized output");

        let schema = output.get("schema").expect("schema");
        assert_eq!(
            schema
                .get("additionalProperties")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            schema
                .get("properties")
                .and_then(|props| props.get("step"))
                .and_then(|step| step.get("additionalProperties"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn anthropic_schema_builder_removes_numeric_bounds() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "properties": {
                    "confidence": {
                        "type": "number",
                        "minimum": 0,
                        "maximum": 1
                    }
                }
            }
        })))
        .expect("sanitized output");

        let confidence = output
            .get("schema")
            .and_then(|schema| schema.get("properties"))
            .and_then(|props| props.get("confidence"))
            .expect("confidence schema");
        assert!(confidence.get("minimum").is_none());
        assert!(confidence.get("maximum").is_none());
    }
}
