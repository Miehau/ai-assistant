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
    complete_openai_compatible_with_options(
        client,
        Some(api_key),
        url,
        model,
        messages,
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
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false
    });

    if let Some(options) = request_options {
        if let Some(key) = options.prompt_cache_key.as_ref() {
            body["prompt_cache_key"] = serde_json::json!(key);
        }
        if let Some(retention) = options.prompt_cache_retention.as_ref() {
            body["prompt_cache_retention"] = serde_json::json!(retention);
        }
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

    let usage = value.get("usage").and_then(|usage| {
        let prompt_tokens = usage
            .get("prompt_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let completion_tokens = usage
            .get("completion_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        if prompt_tokens > 0 || completion_tokens > 0 {
            Some(Usage {
                prompt_tokens,
                completion_tokens,
            })
        } else {
            None
        }
    });

    log::debug!(
        "[llm] provider=openai_compatible model={} content_len={} usage={:?}",
        model,
        content.len(),
        usage
            .as_ref()
            .map(|u| (u.prompt_tokens, u.completion_tokens))
    );

    Ok(StreamResult { content, usage })
}

pub fn stream_openai<F>(
    client: &Client,
    api_key: &str,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    on_chunk: &mut F,
) -> Result<StreamResult, String>
where
    F: FnMut(&str),
{
    stream_openai_with_options(client, api_key, url, model, messages, None, on_chunk)
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

pub fn stream_openai_compatible<F>(
    client: &Client,
    api_key: Option<&str>,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    include_usage: bool,
    on_chunk: &mut F,
) -> Result<StreamResult, String>
where
    F: FnMut(&str),
{
    stream_openai_compatible_with_options(
        client,
        api_key,
        url,
        model,
        messages,
        include_usage,
        None,
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
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true
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
            let prompt_tokens = usage_value
                .get("prompt_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            let completion_tokens = usage_value
                .get("completion_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            if prompt_tokens > 0 || completion_tokens > 0 {
                usage = Some(Usage {
                    prompt_tokens,
                    completion_tokens,
                });
            }
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

pub fn complete_anthropic_with_output_format(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    output_format: Option<Value>,
) -> Result<StreamResult, String> {
    complete_anthropic_with_output_format_with_options(
        client,
        api_key,
        model,
        system,
        messages,
        output_format,
        None,
    )
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

    let has_output_format = output_format.is_some();
    if let Some(output_format_value) = output_format {
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

    let usage = value.get("usage").and_then(|usage| {
        let prompt_tokens = usage
            .get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let completion_tokens = usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        if prompt_tokens > 0 || completion_tokens > 0 {
            Some(Usage {
                prompt_tokens,
                completion_tokens,
            })
        } else {
            None
        }
    });

    log::debug!(
        "[llm] provider=anthropic model={} content_len={} usage={:?}",
        model,
        content.len(),
        usage
            .as_ref()
            .map(|u| (u.prompt_tokens, u.completion_tokens))
    );

    Ok(StreamResult { content, usage })
}

pub fn stream_anthropic<F>(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    on_chunk: &mut F,
) -> Result<StreamResult, String>
where
    F: FnMut(&str),
{
    stream_anthropic_with_options(client, api_key, model, system, messages, None, on_chunk)
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
                        let prompt_tokens = usage_value
                            .get("input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32;
                        let completion_tokens = usage_value
                            .get("output_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32;
                        if prompt_tokens > 0 || completion_tokens > 0 {
                            usage = Some(Usage {
                                prompt_tokens,
                                completion_tokens,
                            });
                        }
                    }
                }
            }

            if event_type == "message_delta" {
                if let Some(usage_value) = value.get("usage") {
                    let output_tokens = usage_value
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    let prompt_tokens = usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0);
                    let completion_tokens = if output_tokens > 0 {
                        output_tokens
                    } else {
                        usage.as_ref().map(|u| u.completion_tokens).unwrap_or(0)
                    };
                    if prompt_tokens > 0 || completion_tokens > 0 {
                        usage = Some(Usage {
                            prompt_tokens,
                            completion_tokens,
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
