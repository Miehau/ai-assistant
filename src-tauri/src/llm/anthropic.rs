use reqwest::blocking::Client;
use serde_json::Value;
use std::io::{BufRead, BufReader};

use super::{compact_error_body, parse_content_blocks, value_to_string, ContentBlock, LlmMessage, LlmRequestOptions, StreamResult, Usage};

const ANTHROPIC_CACHE_BLOCK_MAX_CHARS: usize = 2_500;
const ANTHROPIC_CACHE_CONTROL_MAX_BLOCKS: usize = 4;

#[allow(dead_code)] // used in tests only (schema builder / preflight validation)
pub(crate) fn strip_anthropic_unsupported_schema_keywords(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("$schema");
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

/// Simplify the `thinking` property to avoid Anthropic grammar compilation timeout.
/// Nested arrays (`facts`, `decisions`, `risks`) make the grammar too complex, so
/// replace thinking with a flat object containing only string properties.
#[allow(dead_code)] // used in tests only
fn simplify_thinking_property(schema: &mut Value) {
    let has_complex_thinking = schema
        .get("properties")
        .and_then(|props| props.get("thinking"))
        .and_then(|thinking| thinking.get("properties"))
        .and_then(|thinking_props| thinking_props.as_object())
        .map(|thinking_props| {
            thinking_props.values().any(|prop| {
                prop.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "array")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    if has_complex_thinking {
        if let Some(props) = schema.get_mut("properties") {
            props["thinking"] = serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {"type": "string"},
                    "plan": {"type": "string"},
                    "confidence": {"type": "string"}
                },
                "additionalProperties": false
            });
        }
    }
}

#[allow(dead_code)] // used in tests only
pub(crate) fn build_anthropic_output_schema(raw_schema: Option<Value>) -> Option<Value> {
    let mut schema = raw_schema?;
    strip_anthropic_unsupported_schema_keywords(&mut schema);
    simplify_thinking_property(&mut schema);
    Some(serde_json::json!({
        "type": "json_schema",
        "schema": schema,
    }))
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
        return chunk_text_by_chars(input, ANTHROPIC_CACHE_BLOCK_MAX_CHARS);
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
        blocks.extend(chunk_text_by_chars(segment, ANTHROPIC_CACHE_BLOCK_MAX_CHARS));
    }
    blocks
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
    // Phase 1: Format all messages into Anthropic content blocks.
    // Provider-neutral image blocks are mapped to Anthropic source.base64 format.
    // cache_control may only land on text blocks, so we track the last text block
    // index per message separately from the last block overall.
    let non_system: Vec<&LlmMessage> = messages.iter().filter(|m| m.role != "system").collect();
    let mut formatted: Vec<Value> = Vec::new();
    // (formatted_index, last_text_block_index) — only pushed when the message has
    // at least one text block (image-only messages are skipped for cache marking).
    let mut message_last_text_block_indices: Vec<(usize, usize)> = Vec::new();

    for message in &non_system {
        let neutral_blocks = parse_content_blocks(&message.content);
        let mut content_blocks: Vec<Value> = Vec::new();
        let mut last_text_block_idx: Option<usize> = None;

        for block in neutral_blocks {
            match block {
                ContentBlock::Text(text) => {
                    let chunks = split_anthropic_text_for_cache(&text);
                    for chunk in chunks {
                        if chunk.is_empty() {
                            continue;
                        }
                        last_text_block_idx = Some(content_blocks.len());
                        content_blocks.push(serde_json::json!({
                            "type": "text",
                            "text": chunk
                        }));
                    }
                }
                ContentBlock::Image { media_type, data } => {
                    content_blocks.push(serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data
                        }
                    }));
                }
            }
        }

        if content_blocks.is_empty() {
            content_blocks.push(serde_json::json!({
                "type": "text",
                "text": ""
            }));
        }

        let formatted_idx = formatted.len();
        if let Some(text_idx) = last_text_block_idx {
            message_last_text_block_indices.push((formatted_idx, text_idx));
        }

        formatted.push(serde_json::json!({
            "role": message.role,
            "content": content_blocks
        }));
    }

    // Phase 2: Add cache_control to messages from the END (maximizes cached prefix).
    // Skip the very last message (it's the newest, will change next turn).
    // Work backwards through the remaining messages.
    if cache_enabled && !message_last_text_block_indices.is_empty() {
        let candidates: Vec<(usize, usize)> = if message_last_text_block_indices.len() > 1 {
            // Skip last message, iterate remaining from end
            message_last_text_block_indices[..message_last_text_block_indices.len() - 1]
                .iter()
                .rev()
                .copied()
                .collect()
        } else {
            // Only one message — mark it (better than nothing)
            message_last_text_block_indices.clone()
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

fn build_anthropic_request_body(
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    output_format: Option<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<Value, String> {
    validate_anthropic_output_format(output_format.as_ref())?;
    let sanitized_output_format = output_format;

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

pub fn complete_anthropic(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
) -> Result<StreamResult, String> {
    complete_anthropic_with_options(client, api_key, model, system, messages, None)
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

            if event_type == "error" {
                let error_msg = value
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                let error_type = value
                    .get("error")
                    .and_then(|e| e.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");
                return Err(format!(
                    "Anthropic stream error ({error_type}): {error_msg}"
                ));
            }
        }

        line.clear();
    }

    Ok(StreamResult { content, usage, companion_text: None })
}

fn parse_tool_use_sse<R: std::io::BufRead>(mut reader: R) -> Result<StreamResult, String> {
    use crate::agent::controller_parsing::aggregate_tool_uses;

    let mut line = String::new();
    let mut usage: Option<Usage> = None;
    let mut current_tool_name: Option<String> = None;
    let mut tool_input_buf = String::new();
    // Collect ALL tool_use blocks instead of keeping only the last one.
    let mut collected_tool_uses: Vec<(String, Value)> = Vec::new();
    // Capture text blocks emitted alongside tool calls.
    let mut text_buf = String::new();

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
            Ok(v) => v,
            Err(_) => {
                line.clear();
                continue;
            }
        };

        let event_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "content_block_start" => {
                if let Some(block) = value.get("content_block") {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        current_tool_name =
                            block.get("name").and_then(|v| v.as_str()).map(str::to_string);
                        tool_input_buf.clear();
                    }
                }
            }
            "content_block_delta" => {
                if let Some(delta) = value.get("delta") {
                    let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if current_tool_name.is_some() && delta_type == "input_json_delta" {
                        if let Some(partial) =
                            delta.get("partial_json").and_then(|v| v.as_str())
                        {
                            tool_input_buf.push_str(partial);
                        }
                    } else if current_tool_name.is_none() && delta_type == "text_delta" {
                        if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                            text_buf.push_str(text);
                        }
                    }
                }
            }
            "content_block_stop" => {
                if let Some(name) = current_tool_name.take() {
                    let input: Value = if tool_input_buf.is_empty() {
                        serde_json::json!({})
                    } else {
                        serde_json::from_str(&tool_input_buf).unwrap_or(serde_json::json!({}))
                    };
                    collected_tool_uses.push((name, input));
                    tool_input_buf.clear();
                }
            }
            "message_start" => {
                if let Some(message) = value.get("message") {
                    if let Some(usage_value) = message.get("usage") {
                        usage = parse_anthropic_usage(usage_value);
                    }
                }
            }
            "message_delta" => {
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
            "error" => {
                let error_msg = value
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                let error_type = value
                    .get("error")
                    .and_then(|e| e.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");
                return Err(format!(
                    "Anthropic stream error ({error_type}): {error_msg}"
                ));
            }
            _ => {}
        }

        line.clear();
    }

    // Aggregate collected tool_use blocks (+ text) into a single controller action.
    if !collected_tool_uses.is_empty() {
        let (controller_json, companion_text) =
            aggregate_tool_uses(&collected_tool_uses, &text_buf);
        let result_content = serde_json::to_string(&controller_json).unwrap_or_default();
        return Ok(StreamResult {
            content: result_content,
            usage,
            companion_text,
        });
    }

    // No tool calls: model responded with plain text, indicating task completion.
    // Wrap as a "complete" action so the controller can finish with this response.
    if !text_buf.trim().is_empty() {
        let fallback = serde_json::json!({
            "action": "complete",
            "message": text_buf.trim()
        });
        let result_content = serde_json::to_string(&fallback).unwrap_or_default();
        return Ok(StreamResult {
            content: result_content,
            usage,
            companion_text: None,
        });
    }

    Err("Anthropic returned empty response (no tool call or text content)".to_string())
}

pub fn complete_anthropic_with_tools(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    tools: Vec<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<StreamResult, String> {
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
        "max_tokens": 8192,
        "temperature": 0,
        "tools": tools,
        "tool_choice": { "type": "auto" },
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
        let error_body = compact_error_body(response.text().unwrap_or_default());
        return Err(format!("Anthropic error: {status} - {error_body}"));
    }

    parse_tool_use_sse(BufReader::new(response))
}

// ---------------------------------------------------------------------------
// Provider struct implementing LlmProvider trait
// ---------------------------------------------------------------------------

use super::traits::LlmProvider;
use crate::agent::controller_parsing::controller_tool_definitions;
use crate::agent::prompts::{CONTROLLER_PROMPT_ANTHROPIC, CONTROLLER_PROMPT_BASE};

/// Anthropic provider (Claude API via native messages endpoint).
pub struct AnthropicProvider {
    pub api_key: String,
    pub model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn controller_call(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        output_format: Option<Value>,
        request_options: Option<&LlmRequestOptions>,
    ) -> Result<StreamResult, String> {
        // Anthropic does NOT prepend system prompt into messages — it passes
        // the system prompt as a separate parameter. Messages stay as-is.
        if output_format.is_some() {
            // Controller call: use native tool calling instead of structured output.
            // Swap CONTROLLER_PROMPT_BASE for CONTROLLER_PROMPT_ANTHROPIC in messages.
            let controller_messages: Vec<LlmMessage> = messages
                .iter()
                .map(|m| {
                    if m.role == "system"
                        && m.content.as_str() == Some(CONTROLLER_PROMPT_BASE)
                    {
                        LlmMessage {
                            role: "system".to_string(),
                            content: serde_json::json!(CONTROLLER_PROMPT_ANTHROPIC),
                        }
                    } else {
                        m.clone()
                    }
                })
                .collect();
            complete_anthropic_with_tools(
                client,
                &self.api_key,
                &self.model,
                system_prompt,
                &controller_messages,
                controller_tool_definitions(),
                request_options,
            )
        } else {
            complete_anthropic_with_output_format_with_options(
                client,
                &self.api_key,
                &self.model,
                system_prompt,
                messages,
                None,
                request_options,
            )
        }
    }

    fn stream_response(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        request_options: Option<&LlmRequestOptions>,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<StreamResult, String> {
        // Anthropic passes system prompt separately — do NOT prepend to messages.
        stream_anthropic_with_options(
            client,
            &self.api_key,
            &self.model,
            system_prompt,
            messages,
            request_options,
            &mut |s| on_chunk(s),
        )
    }

    fn complete(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
    ) -> Result<StreamResult, String> {
        // Anthropic passes system prompt separately — do NOT prepend to messages.
        complete_anthropic(client, &self.api_key, &self.model, system_prompt, messages)
    }

    fn build_request_options(&self, _conversation_id: &str, _phase: &str) -> LlmRequestOptions {
        LlmRequestOptions {
            prompt_cache_key: None,
            prompt_cache_retention: None,
            anthropic_cache_breakpoints: vec![0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn anthropic_schema_builder_removes_if_then_allof() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "object",
            "properties": { "action": { "type": "string" } },
            "allOf": [
                {
                    "if": { "properties": { "action": { "const": "next_step" } } },
                    "then": { "required": ["step", "thinking"] }
                }
            ]
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
            "type": "object",
            "oneOf": [
                {
                    "properties": { "action": { "const": "next_step" } },
                    "required": ["action", "step", "thinking"]
                }
            ]
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
        let raw = json!({
            "type": "object",
            "oneOf": [
                {
                    "properties": { "action": { "const": "next_step" } },
                    "required": ["action"]
                }
            ]
        });
        let wrapped = build_anthropic_output_schema(Some(raw));
        let body =
            build_anthropic_request_body("claude-sonnet", None, &messages, wrapped, None)
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
    fn anthropic_request_body_rejects_non_object_raw_schema() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let wrapped = build_anthropic_output_schema(Some(json!("not an object")));
        let err = build_anthropic_request_body(
            "claude-sonnet",
            None,
            &messages,
            wrapped,
            None,
        )
        .expect_err("should reject");
        assert!(err.contains("must be an object"));
    }

    #[test]
    fn anthropic_output_format_preflight_accepts_valid_schema() {
        let format = json!({
            "type": "json_schema",
            "schema": {
                "type": "object",
                "properties": { "action": { "type": "string" } }
            }
        });
        validate_anthropic_output_format(Some(&format)).expect("should pass preflight");
    }

    #[test]
    fn anthropic_schema_builder_adds_additional_properties_false_recursively() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "object",
            "properties": {
                "step": {
                    "type": "object",
                    "properties": {
                        "description": { "type": "string" }
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
            "type": "object",
            "properties": {
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1
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

    #[test]
    fn anthropic_schema_builder_simplifies_thinking_with_arrays() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "object",
            "properties": {
                "action": { "type": "string" },
                "thinking": {
                    "type": "object",
                    "properties": {
                        "facts": { "type": "array", "items": { "type": "string" } },
                        "decisions": { "type": "array", "items": { "type": "string" } },
                        "risks": { "type": "array", "items": { "type": "string" } }
                    }
                }
            }
        })))
        .expect("sanitized output");

        let thinking = output
            .get("schema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.get("thinking"))
            .expect("thinking property");

        // Should be simplified to string-only properties
        let props = thinking.get("properties").expect("thinking.properties");
        assert!(props.get("task").is_some(), "should have task");
        assert!(props.get("plan").is_some(), "should have plan");
        assert!(props.get("confidence").is_some(), "should have confidence");
        assert!(props.get("facts").is_none(), "should NOT have facts");
        assert!(props.get("decisions").is_none(), "should NOT have decisions");
        assert!(props.get("risks").is_none(), "should NOT have risks");
    }

    #[test]
    fn anthropic_schema_builder_preserves_simple_thinking() {
        let output = build_anthropic_output_schema(Some(json!({
            "type": "object",
            "properties": {
                "thinking": {
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string" },
                        "plan": { "type": "string" }
                    }
                }
            }
        })))
        .expect("sanitized output");

        let thinking = output
            .get("schema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.get("thinking"))
            .expect("thinking property");

        // Should NOT be simplified since there are no array properties
        let props = thinking.get("properties").expect("thinking.properties");
        assert!(props.get("summary").is_some(), "should still have summary");
        assert!(props.get("plan").is_some(), "should still have plan");
    }

    #[test]
    fn tool_use_sse_parses_call_tool() {
        use std::io::Cursor;
        // Build SSE lines programmatically to avoid escaping pitfalls.
        let tool_input = json!({"tool": "weather", "args": {"city": "NYC"}});
        let partial_json = serde_json::to_string(&tool_input).unwrap();
        let mut sse = String::new();
        sse.push_str("data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":100,\"output_tokens\":0}}}\n\n");
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"call_tool\",\"input\":{}}}\n\n");
        let delta_event = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "input_json_delta",
                "partial_json": partial_json
            }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&delta_event).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");
        sse.push_str("data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":50}}\n\n");
        let cursor = Cursor::new(sse);
        let result = parse_tool_use_sse(cursor).expect("parse");
        let value: serde_json::Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(value["action"], "next_step");
        assert_eq!(value["tool"], "weather");
        assert_eq!(value["args"]["city"], "NYC");
        assert!(result.usage.is_some());
    }

    #[test]
    fn tool_use_sse_parses_complete() {
        use std::io::Cursor;
        let tool_input = json!({"message": "All done"});
        let partial_json = serde_json::to_string(&tool_input).unwrap();
        let mut sse = String::new();
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_2\",\"name\":\"complete\",\"input\":{}}}\n\n");
        let delta_event = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "input_json_delta",
                "partial_json": partial_json
            }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&delta_event).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");
        let cursor = Cursor::new(sse);
        let result = parse_tool_use_sse(cursor).expect("parse");
        let value: serde_json::Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(value["action"], "complete");
        assert_eq!(value["message"], "All done");
    }

    #[test]
    fn tool_use_sse_falls_back_to_text_block_when_message_empty() {
        use std::io::Cursor;
        // Model emits a text block with the response, then calls respond with no message.
        // This happens with vision models that describe the image in a text block.
        let mut sse = String::new();
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n");
        let text_delta = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "Here is what I see in the image." }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&text_delta).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"respond\",\"input\":{}}}\n\n");
        // Tool input has no message field
        let input_delta = json!({
            "type": "content_block_delta",
            "index": 1,
            "delta": { "type": "input_json_delta", "partial_json": "{\"thinking\":{}}" }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&input_delta).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":1}\n\n");

        let cursor = Cursor::new(sse);
        let result = parse_tool_use_sse(cursor).expect("parse");
        let value: serde_json::Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(value["action"], "next_step");
        assert_eq!(value["type"], "respond");
        assert_eq!(
            value["message"],
            "Here is what I see in the image.",
            "should fall back to text block content"
        );
    }

    #[test]
    fn tool_use_sse_text_only_response_synthesizes_respond_action() {
        use std::io::Cursor;
        // Model ignores tool_choice:"any" and responds with only a text block (no tool_use).
        // This can happen with vision input. The parser should synthesize a respond action.
        let mut sse = String::new();
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n");
        let text_delta = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "The image shows a cat." }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&text_delta).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");
        // No tool_use block at all — just message_delta with usage
        sse.push_str("data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":10}}\n\n");

        let cursor = Cursor::new(sse);
        let result = parse_tool_use_sse(cursor).expect("parse");
        let value: serde_json::Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(value["action"], "next_step");
        assert_eq!(value["type"], "respond");
        assert_eq!(value["message"], "The image shows a cat.");
    }

    #[test]
    fn tool_use_sse_empty_response_returns_error() {
        use std::io::Cursor;
        // Model returns nothing — no text, no tool_use. Should be an error, not empty JSON.
        let sse = "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":0}}\n\n";
        let cursor = Cursor::new(sse);
        let result = parse_tool_use_sse(cursor);
        assert!(result.is_err(), "empty response should be an error");
        assert!(result.unwrap_err().contains("empty response"));
    }

    #[test]
    fn anthropic_image_block_mapped_to_source_base64() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!([
                { "type": "image", "media_type": "image/png", "data": "abc123" },
                { "type": "text", "text": "what is this?" }
            ]),
        }];
        let mut count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut count, false);

        let blocks = formatted[0]["content"].as_array().expect("content array");
        // First block should be the image in Anthropic source.base64 format
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["type"], "base64");
        assert_eq!(blocks[0]["source"]["media_type"], "image/png");
        assert_eq!(blocks[0]["source"]["data"], "abc123");
        // Second block should be the text
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "what is this?");
    }

    #[test]
    fn anthropic_cache_control_lands_on_text_block_not_image() {
        // Message ends with an image — cache_control must go on the last text block
        let messages = vec![
            LlmMessage {
                role: "user".to_string(),
                content: json!([
                    { "type": "text", "text": "look at this" },
                    { "type": "image", "media_type": "image/jpeg", "data": "imgdata" }
                ]),
            },
            LlmMessage {
                role: "assistant".to_string(),
                content: json!("I see an image."),
            },
        ];
        let mut count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut count, true);

        // First message (index 0) should have cache_control on its text block,
        // NOT on the trailing image block.
        let blocks = formatted[0]["content"].as_array().expect("content array");
        let text_block = &blocks[0];
        let image_block = &blocks[1];
        assert_eq!(
            text_block
                .get("cache_control")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("ephemeral"),
            "cache_control should be on the text block"
        );
        assert!(
            image_block.get("cache_control").is_none(),
            "image block must NOT have cache_control"
        );
    }

    #[test]
    fn anthropic_image_only_message_skipped_for_cache() {
        // A message with only image blocks should not receive cache_control at all
        let messages = vec![
            LlmMessage {
                role: "user".to_string(),
                content: json!([
                    { "type": "image", "media_type": "image/png", "data": "x" }
                ]),
            },
            LlmMessage {
                role: "assistant".to_string(),
                content: json!("response"),
            },
        ];
        let mut count = 0usize;
        let formatted = format_anthropic_messages(&messages, &mut count, true);

        let image_msg_blocks = formatted[0]["content"].as_array().expect("content array");
        assert!(
            image_msg_blocks
                .iter()
                .all(|b| b.get("cache_control").is_none()),
            "image-only message should not have cache_control"
        );
    }

    // --- Multi-block SSE tests ---

    #[test]
    fn tool_use_sse_multiple_call_tool_merged_into_batch() {
        use std::io::Cursor;
        // Model emits two call_tool blocks — should be merged into a tool_batch.
        let mut sse = String::new();
        sse.push_str("data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":50,\"output_tokens\":0}}}\n\n");

        // First call_tool
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"call_tool\",\"input\":{}}}\n\n");
        let delta1 = json!({
            "type": "content_block_delta", "index": 0,
            "delta": { "type": "input_json_delta", "partial_json": "{\"tool\":\"weather\",\"args\":{\"city\":\"NYC\"}}" }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&delta1).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");

        // Second call_tool
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_2\",\"name\":\"call_tool\",\"input\":{}}}\n\n");
        let delta2 = json!({
            "type": "content_block_delta", "index": 1,
            "delta": { "type": "input_json_delta", "partial_json": "{\"tool\":\"weather\",\"args\":{\"city\":\"LA\"}}" }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&delta2).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":1}\n\n");

        let cursor = Cursor::new(sse);
        let result = parse_tool_use_sse(cursor).expect("parse");
        let value: serde_json::Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(value["action"], "next_step");
        assert_eq!(value["type"], "tool_batch");
        let tools = value["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["tool"], "weather");
        assert_eq!(tools[0]["args"]["city"], "NYC");
        assert_eq!(tools[1]["args"]["city"], "LA");
        assert!(result.companion_text.is_none());
    }

    #[test]
    fn tool_use_sse_text_plus_call_tool_produces_companion_text() {
        use std::io::Cursor;
        // Model emits a text block followed by a call_tool block.
        let mut sse = String::new();

        // Text block
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n");
        let text_delta = json!({
            "type": "content_block_delta", "index": 0,
            "delta": { "type": "text_delta", "text": "Let me check the weather for you." }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&text_delta).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");

        // Tool call
        sse.push_str("data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"call_tool\",\"input\":{}}}\n\n");
        let tool_delta = json!({
            "type": "content_block_delta", "index": 1,
            "delta": { "type": "input_json_delta", "partial_json": "{\"tool\":\"weather\",\"args\":{\"city\":\"NYC\"}}" }
        });
        sse.push_str(&format!("data: {}\n\n", serde_json::to_string(&tool_delta).unwrap()));
        sse.push_str("data: {\"type\":\"content_block_stop\",\"index\":1}\n\n");

        let cursor = Cursor::new(sse);
        let result = parse_tool_use_sse(cursor).expect("parse");
        let value: serde_json::Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(value["action"], "next_step");
        assert_eq!(value["type"], "tool");
        assert_eq!(value["tool"], "weather");
        assert_eq!(
            result.companion_text.as_deref(),
            Some("Let me check the weather for you."),
            "text block should become companion_text"
        );
    }
}
