use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::process::Command;

const PROVIDER_ERROR_BODY_MAX_CHARS: usize = 2_000;
const ANTHROPIC_CACHE_BLOCK_MAX_CHARS: usize = 2_500;
const ANTHROPIC_CACHE_CONTROL_MAX_BLOCKS: usize = 4;

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

fn validate_openai_output_format(output_format: Option<&Value>) -> Result<(), String> {
    let Some(output_format) = output_format else {
        return Ok(());
    };

    let format_type = output_format
        .get("type")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "OpenAI preflight: output_format.type must be \"json_schema\"".to_string())?;
    if format_type != "json_schema" {
        return Err(format!(
            "OpenAI preflight: unsupported output_format.type \"{format_type}\""
        ));
    }

    let schema = output_format
        .get("schema")
        .or_else(|| output_format.get("json_schema").and_then(|value| value.get("schema")))
        .ok_or_else(|| "OpenAI preflight: missing output_format.schema".to_string())?;
    if !schema.is_object() {
        return Err("OpenAI preflight: output_format.schema must be an object".to_string());
    }

    Ok(())
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

fn parse_openai_compatible_response(value: &Value) -> StreamResult {
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

    StreamResult { content, usage }
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

pub(crate) fn strip_anthropic_unsupported_schema_keywords(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("if");
            map.remove("then");
            map.remove("else");
            map.remove("allOf");
            map.remove("oneOf");
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
    let output = output_format?;

    // Start with any existing json_schema block
    let mut json_schema = if let Some(existing) = output.get("json_schema") {
        existing.clone()
    } else {
        let schema_value = output.get("schema")?.clone();
        serde_json::json!({ "schema": schema_value })
    };

    // Ensure the schema field is populated
    if json_schema.get("schema").is_none() {
        if let Some(schema_value) = output.get("schema").cloned() {
            json_schema["schema"] = schema_value;
        }
    }

    // Derive the name, preferring explicit json_schema.name > output.name
    let name = json_schema
        .get("name")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .or_else(|| {
            output
                .get("name")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| "response".to_string());
    json_schema["name"] = Value::String(name);

    // Default to strict JSON schema enforcement
    if json_schema.get("strict").is_none() {
        json_schema["strict"] = Value::Bool(true);
    }

    Some(serde_json::json!({
        "type": "json_schema",
        "json_schema": json_schema,
    }))
}

fn build_openai_request_body(
    model: &str,
    messages: &[LlmMessage],
    stream: bool,
    include_usage: bool,
    output_format: Option<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<Value, String> {
    let mut body = build_openai_compatible_body(model, messages, stream, include_usage, request_options);

    if let Some(format_value) = output_format.as_ref() {
        validate_openai_output_format(Some(format_value))?;
    }
    if let Some(openai_output_schema) = build_openai_output_schema(output_format) {
        body["response_format"] = openai_output_schema;
    }

    Ok(body)
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

pub(crate) fn validate_anthropic_output_format(
    output_format: Option<&Value>,
) -> Result<(), String> {
    let Some(output_format) = output_format else {
        return Ok(());
    };

    let format_type = output_format
        .get("type")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            "Anthropic preflight: output_format.type must be \"json_schema\"".to_string()
        })?;
    if format_type != "json_schema" {
        return Err(format!(
            "Anthropic preflight: unsupported output_format.type \"{format_type}\""
        ));
    }

    let schema = output_format
        .get("schema")
        .ok_or_else(|| "Anthropic preflight: missing output_format.schema".to_string())?;
    if !schema.is_object() {
        return Err("Anthropic preflight: output_format.schema must be an object".to_string());
    }

    Ok(())
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
    let body = build_openai_request_body(
        model,
        messages,
        false,
        false,
        output_format,
        request_options,
    )?;

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
    let result = parse_openai_compatible_response(&value);

    log::debug!(
        "[llm] provider=openai_compatible model={} content_len={} usage={:?}",
        model,
        result.content.len(),
        result.usage.as_ref().map(|u| {
            (
                u.prompt_tokens,
                u.completion_tokens,
                u.cached_prompt_tokens,
                u.cache_read_input_tokens,
                u.cache_creation_input_tokens,
            )
        })
    );

    Ok(result)
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

fn split_anthropic_system_prefix(
    system: Option<&str>,
    messages: &[LlmMessage],
) -> (Option<String>, Vec<LlmMessage>) {
    let mut sections: Vec<String> = Vec::new();
    let mut append_section = |candidate: &str| {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            return;
        }
        if sections.last().map(|last| last == trimmed).unwrap_or(false) {
            return;
        }
        sections.push(trimmed.to_string());
    };

    if let Some(system_text) = system {
        append_section(system_text);
    }

    let mut message_start_index = 0usize;
    for message in messages {
        if message.role != "system" {
            break;
        }
        let text = value_to_string(&message.content);
        append_section(&text);
        message_start_index += 1;
    }

    let merged_system = if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    };
    let remaining_messages = messages[message_start_index..].to_vec();

    (merged_system, remaining_messages)
}

fn split_anthropic_text_for_cache(input: &str) -> Vec<(String, bool)> {
    let mut marker_positions = [
        "\nSTATE SUMMARY:\n",
        "\nLAST TOOL OUTPUT:\n",
        "\nLIMITS:\n",
        "\nUSER REQUEST:\n",
        "\nRECENT MESSAGES:\n",
        "\nTOOL OUTPUTS (if any):\n",
        "\nCONTROLLER DRAFT (if any):\n",
        "\nInstructions:\n",
    ]
    .iter()
    .filter_map(|candidate| input.find(candidate))
    .collect::<Vec<_>>();

    if marker_positions.is_empty() {
        return chunk_text_by_chars(input, ANTHROPIC_CACHE_BLOCK_MAX_CHARS)
            .into_iter()
            .map(|chunk| (chunk, false))
            .collect();
    }

    marker_positions.sort_unstable();
    marker_positions.dedup();

    let mut segments = Vec::new();
    let mut start = 0usize;
    for marker_start in marker_positions {
        if marker_start > start {
            segments.push(&input[start..marker_start]);
        }
        start = marker_start;
    }
    if start < input.len() {
        segments.push(&input[start..]);
    }

    let mut blocks = Vec::new();
    for segment in segments {
        let chunks = chunk_text_by_chars(segment, ANTHROPIC_CACHE_BLOCK_MAX_CHARS);
        let chunk_count = chunks.len();
        for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
            blocks.push((chunk, chunk_count > 0 && chunk_idx + 1 == chunk_count));
        }
    }
    blocks
}

fn is_anthropic_cache_enabled(request_options: Option<&LlmRequestOptions>) -> bool {
    request_options
        .map(|options| !options.anthropic_cache_breakpoints.is_empty())
        .unwrap_or(false)
}

fn format_anthropic_system(
    system: Option<&str>,
    cache_control_count: &mut usize,
    cache_enabled: bool,
) -> Option<Value> {
    let text = system?;
    if text.trim().is_empty() {
        return None;
    }

    let mut content_blocks = Vec::new();
    let chunks = chunk_text_by_chars(text, ANTHROPIC_CACHE_BLOCK_MAX_CHARS);
    let chunk_count = chunks.len();
    for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
        if chunk.is_empty() {
            continue;
        }
        let mut block = serde_json::json!({
            "type": "text",
            "text": chunk
        });
        // Only mark the LAST system block — caches the entire system prompt prefix
        if cache_enabled
            && chunk_idx + 1 == chunk_count
            && *cache_control_count < ANTHROPIC_CACHE_CONTROL_MAX_BLOCKS
        {
            block["cache_control"] = serde_json::json!({ "type": "ephemeral" });
            *cache_control_count += 1;
        }
        content_blocks.push(block);
    }

    if content_blocks.is_empty() {
        None
    } else {
        Some(Value::Array(content_blocks))
    }
}

fn format_anthropic_messages(
    messages: &[LlmMessage],
    cache_control_count: &mut usize,
    cache_enabled: bool,
) -> Vec<Value> {
    // Phase 1: Format all messages into blocks without cache_control
    let non_system: Vec<&LlmMessage> = messages.iter().filter(|m| m.role != "system").collect();
    let mut formatted: Vec<Value> = Vec::new();
    // Track (formatted_index, last_block_index) for each message so we can add
    // cache_control to the last block of selected messages in phase 2.
    let mut message_last_block_indices: Vec<(usize, usize)> = Vec::new();

    for message in &non_system {
        let text = value_to_string(&message.content);
        let chunks = split_anthropic_text_for_cache(&text);
        let mut content_blocks = Vec::new();

        for (chunk, _section_boundary) in chunks {
            if chunk.is_empty() {
                continue;
            }
            content_blocks.push(serde_json::json!({
                "type": "text",
                "text": chunk
            }));
        }

        if content_blocks.is_empty() {
            content_blocks.push(serde_json::json!({
                "type": "text",
                "text": ""
            }));
        }

        let last_block_idx = content_blocks.len().saturating_sub(1);
        let formatted_idx = formatted.len();
        message_last_block_indices.push((formatted_idx, last_block_idx));

        formatted.push(serde_json::json!({
            "role": message.role,
            "content": content_blocks
        }));
    }

    // Phase 2: Add cache_control to messages from the END (maximizes cached prefix).
    // Skip the very last message (it's the newest, will change next turn).
    // Work backwards through the remaining messages.
    if cache_enabled && !message_last_block_indices.is_empty() {
        let candidates: Vec<(usize, usize)> = if message_last_block_indices.len() > 1 {
            // Skip last message, iterate remaining from end
            message_last_block_indices[..message_last_block_indices.len() - 1]
                .iter()
                .rev()
                .copied()
                .collect()
        } else {
            // Only one message — mark it (better than nothing)
            message_last_block_indices.clone()
        };

        for (fmt_idx, block_idx) in candidates {
            if *cache_control_count >= ANTHROPIC_CACHE_CONTROL_MAX_BLOCKS {
                break;
            }
            if let Some(msg) = formatted.get_mut(fmt_idx) {
                if let Some(blocks) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
                    if let Some(block) = blocks.get_mut(block_idx) {
                        block["cache_control"] = serde_json::json!({ "type": "ephemeral" });
                        *cache_control_count += 1;
                    }
                }
            }
        }
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

fn build_anthropic_request_body(
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    output_format: Option<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<Value, String> {
    let sanitized_output_format = build_anthropic_output_schema(output_format);
    validate_anthropic_output_format(sanitized_output_format.as_ref())?;

    let cache_enabled = is_anthropic_cache_enabled(request_options);
    let (merged_system, anthropic_messages) = split_anthropic_system_prefix(system, messages);
    let mut cache_control_count = 0usize;
    let formatted_system = format_anthropic_system(
        merged_system.as_deref(),
        &mut cache_control_count,
        cache_enabled,
    );
    let formatted_messages = format_anthropic_messages(
        &anthropic_messages,
        &mut cache_control_count,
        cache_enabled,
    );

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

    if let Some(output_format_value) = sanitized_output_format {
        body["output_config"] = serde_json::json!({
            "format": output_format_value
        });
    }

    Ok(body)
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
    let mut body = build_anthropic_request_body(
        model,
        system,
        messages,
        output_format,
        request_options,
    )?;

    // Use streaming to avoid HTTP timeout on long-running structured output requests.
    body["stream"] = serde_json::json!(true);

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
        let error_body = compact_error_body(response.text().unwrap_or_default());
        return Err(format!("Anthropic error: {status} - {error_body}"));
    }

    // Collect streamed chunks silently (no on_chunk callback needed for controller).
    let mut noop = |_: &str| {};
    read_anthropic_sse(response, &mut noop)
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
    let cache_enabled = is_anthropic_cache_enabled(request_options);
    let (merged_system, anthropic_messages) = split_anthropic_system_prefix(system, messages);
    let mut cache_control_count = 0usize;
    let formatted_system = format_anthropic_system(
        merged_system.as_deref(),
        &mut cache_control_count,
        cache_enabled,
    );
    let formatted_messages = format_anthropic_messages(
        &anthropic_messages,
        &mut cache_control_count,
        cache_enabled,
    );

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

    read_anthropic_sse(response, on_chunk)
}

fn read_anthropic_sse<F>(
    response: reqwest::blocking::Response,
    on_chunk: &mut F,
) -> Result<StreamResult, String>
where
    F: FnMut(&str),
{
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
                    } else if delta_type == "json_delta" {
                        if let Some(text) = delta.get("partial_json").and_then(|v| v.as_str()) {
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
        let body =
            build_openai_compatible_body("gpt-5-mini", &messages, true, true, Some(&options));

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
    fn anthropic_system_prefix_lifts_leading_system_messages() {
        let messages = vec![
            LlmMessage {
                role: "system".to_string(),
                content: json!("base instructions"),
            },
            LlmMessage {
                role: "system".to_string(),
                content: json!("tool list"),
            },
            LlmMessage {
                role: "user".to_string(),
                content: json!("hello"),
            },
        ];

        let (merged, remaining) = split_anthropic_system_prefix(None, &messages);
        assert_eq!(merged.as_deref(), Some("base instructions\n\ntool list"));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].role, "user");
    }

    #[test]
    fn anthropic_system_prefix_deduplicates_adjacent_identical_sections() {
        let messages = vec![
            LlmMessage {
                role: "system".to_string(),
                content: json!("base instructions"),
            },
            LlmMessage {
                role: "system".to_string(),
                content: json!("runtime constraints"),
            },
            LlmMessage {
                role: "user".to_string(),
                content: json!("hello"),
            },
        ];

        let (merged, remaining) =
            split_anthropic_system_prefix(Some("base instructions"), &messages);
        assert_eq!(
            merged.as_deref(),
            Some("base instructions\n\nruntime constraints")
        );
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].role, "user");
    }

    #[test]
    fn anthropic_format_omits_cache_when_disabled() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello world"),
        }];
        let mut cache_control_count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut cache_control_count, false);
        let blocks = formatted[0]["content"].as_array().expect("content array");
        assert!(
            blocks
                .iter()
                .all(|block| block.get("cache_control").is_none()),
            "cache_control markers should be absent when cache is disabled"
        );
    }

    #[test]
    fn anthropic_format_marks_last_message_before_newest() {
        // With 3 messages, cache should be on the second-to-last (index 1),
        // NOT on the last (index 2) which is the newest/changing message.
        let messages = vec![
            LlmMessage {
                role: "user".to_string(),
                content: json!("first message"),
            },
            LlmMessage {
                role: "assistant".to_string(),
                content: json!("second message"),
            },
            LlmMessage {
                role: "user".to_string(),
                content: json!("third newest message"),
            },
        ];
        let mut cache_control_count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut cache_control_count, true);

        // Message at index 1 (second-to-last) should have cache_control
        let msg1_blocks = formatted[1]["content"].as_array().expect("content array");
        let last_block = msg1_blocks.last().unwrap();
        assert_eq!(
            last_block
                .get("cache_control")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("ephemeral"),
            "second-to-last message should have cache_control"
        );

        // Last message (newest) should NOT have cache_control
        let msg2_blocks = formatted[2]["content"].as_array().expect("content array");
        assert!(
            msg2_blocks
                .iter()
                .all(|block| block.get("cache_control").is_none()),
            "newest message should NOT have cache_control"
        );
    }

    #[test]
    fn anthropic_format_single_message_gets_cache() {
        // With only 1 message, it should still get cached (better than nothing)
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("only message"),
        }];
        let mut cache_control_count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut cache_control_count, true);
        let blocks = formatted[0]["content"].as_array().expect("content array");
        assert_eq!(
            blocks
                .last()
                .unwrap()
                .get("cache_control")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("ephemeral")
        );
    }

    #[test]
    fn anthropic_system_only_marks_last_block() {
        let long_system = "a".repeat(ANTHROPIC_CACHE_BLOCK_MAX_CHARS * 3);
        let mut cache_control_count = 0usize;
        let system_blocks =
            format_anthropic_system(Some(&long_system), &mut cache_control_count, true)
                .expect("system blocks");
        let blocks = system_blocks.as_array().expect("array");
        assert!(blocks.len() >= 3, "should have multiple blocks");

        // Only the last block should have cache_control
        for (i, block) in blocks.iter().enumerate() {
            if i + 1 == blocks.len() {
                assert!(
                    block.get("cache_control").is_some(),
                    "last system block should have cache_control"
                );
            } else {
                assert!(
                    block.get("cache_control").is_none(),
                    "non-last system block should NOT have cache_control"
                );
            }
        }
        assert_eq!(cache_control_count, 1);
    }

    #[test]
    fn anthropic_format_limits_cache_control_blocks_per_request() {
        // With many messages, total cache_control markers should not exceed the limit
        let messages: Vec<LlmMessage> = (0..10)
            .map(|i| LlmMessage {
                role: if i % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                content: json!(format!("message {i}")),
            })
            .collect();
        let mut cache_control_count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut cache_control_count, true);

        let marked: usize = formatted
            .iter()
            .filter_map(|message| message.get("content").and_then(|v| v.as_array()))
            .flat_map(|blocks| blocks.iter())
            .filter(|block| block.get("cache_control").is_some())
            .count();

        assert!(
            marked <= ANTHROPIC_CACHE_CONTROL_MAX_BLOCKS,
            "expected no more than {} cache_control blocks, found {marked}",
            ANTHROPIC_CACHE_CONTROL_MAX_BLOCKS
        );
    }

    #[test]
    fn anthropic_cache_breakpoints_placed_near_end_of_conversation() {
        // With 10 messages and cache enabled, breakpoints should be on messages
        // near the end (not at the beginning)
        let messages: Vec<LlmMessage> = (0..10)
            .map(|i| LlmMessage {
                role: if i % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                content: json!(format!("message {i}")),
            })
            .collect();
        let mut cache_control_count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut cache_control_count, true);

        // First few messages should NOT have cache_control
        for i in 0..5 {
            let blocks = formatted[i]["content"].as_array().expect("content array");
            assert!(
                blocks
                    .iter()
                    .all(|block| block.get("cache_control").is_none()),
                "early message {i} should NOT have cache_control"
            );
        }

        // Last message (index 9) should NOT have cache_control (it's the newest)
        let last_blocks = formatted[9]["content"].as_array().expect("content array");
        assert!(
            last_blocks
                .iter()
                .all(|block| block.get("cache_control").is_none()),
            "newest message should NOT have cache_control"
        );

        // Messages near the end (but not the last) should have cache_control
        assert!(
            cache_control_count > 0,
            "should have placed some cache breakpoints"
        );
    }

    #[test]
    fn complete_openai_compatible_parses_cache_metrics_from_mock_response() {
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
        });

        let result = parse_openai_compatible_response(&response_body);
        let usage = result.usage.expect("usage");
        assert_eq!(usage.prompt_tokens, 1000);
        assert_eq!(usage.completion_tokens, 25);
        assert_eq!(usage.cached_prompt_tokens, 800);
        assert_eq!(result.content, "ok");
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
        let expected = json!({
            "type": "json_schema",
            "json_schema": {
                "schema": source.get("schema").cloned().expect("schema"),
                "name": "response",
                "strict": true
            }
        });
        assert_eq!(built, expected);
    }

    #[test]
    fn openai_request_body_includes_response_format_with_strict() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let format = json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "properties": {
                    "ok": { "type": "string" }
                }
            }
        });
        let body = build_openai_request_body(
            "gpt-5-mini",
            &messages,
            false,
            false,
            Some(format),
            None,
        )
        .expect("body");
        let response_format = body.get("response_format").expect("response_format");
        let json_schema = response_format
            .get("json_schema")
            .expect("json_schema");
        assert_eq!(
            json_schema.get("strict").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn openai_request_body_omits_response_format_when_none() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let body =
            build_openai_request_body("gpt-5-mini", &messages, false, false, None, None)
                .expect("body");
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn openai_request_body_rejects_invalid_output_format() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let err = build_openai_request_body(
            "gpt-5-mini",
            &messages,
            false,
            false,
            Some(json!({
                "type": "json_object",
                "schema": { "type": "object" }
            })),
            None,
        )
        .expect_err("should reject");
        assert!(err.contains("unsupported output_format.type"));
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
    fn anthropic_schema_builder_removes_one_of_for_compatibility() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "oneOf": [
                    {
                        "properties": { "action": { "const": "next_step" } },
                        "required": ["action", "step", "thinking"]
                    }
                ]
            }
        })))
        .expect("sanitized output");

        let schema = output.get("schema").expect("schema");
        assert!(
            schema.get("oneOf").is_none(),
            "output_format.schema.oneOf must be removed for Anthropic compatibility"
        );
    }

    #[test]
    fn anthropic_request_body_includes_sanitized_output_format() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let format = json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "oneOf": [
                    {
                        "properties": { "action": { "const": "next_step" } },
                        "required": ["action"]
                    }
                ]
            }
        });
        let body =
            build_anthropic_request_body("claude-sonnet", None, &messages, Some(format), None)
                .expect("body");
        let output_config = body.get("output_config").expect("output_config");
        let format_obj = output_config.get("format").expect("format");
        let schema = format_obj.get("schema").expect("schema");
        assert!(schema.get("oneOf").is_none());
        assert_eq!(
            schema
                .get("additionalProperties")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn anthropic_output_format_preflight_rejects_unsupported_type() {
        let err = validate_anthropic_output_format(Some(&json!({
            "type": "json_object",
            "schema": { "type": "object" }
        })))
        .expect_err("should reject");
        assert!(err.contains("unsupported output_format.type"));
    }

    #[test]
    fn anthropic_output_format_preflight_rejects_missing_schema() {
        let err = validate_anthropic_output_format(Some(&json!({
            "type": "json_schema"
        })))
        .expect_err("should reject");
        assert!(err.contains("missing output_format.schema"));
    }

    #[test]
    fn anthropic_request_body_rejects_invalid_output_format() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let err = build_anthropic_request_body(
            "claude-sonnet",
            None,
            &messages,
            Some(json!({ "type": "json_object" })),
            None,
        )
        .expect_err("should reject");
        assert!(err.contains("unsupported output_format.type"));
    }

    #[test]
    fn anthropic_output_format_preflight_accepts_valid_schema() {
        let format = json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string" }
                }
            }
        });
        validate_anthropic_output_format(Some(&format)).expect("should pass preflight");
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
