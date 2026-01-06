use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};

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

pub fn stream_openai(
    client: &Client,
    api_key: &str,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    on_chunk: impl FnMut(&str),
) -> Result<StreamResult, String> {
    stream_openai_compatible(
        client,
        Some(api_key),
        url,
        model,
        messages,
        Some(serde_json::json!({ "include_usage": true })),
        on_chunk,
    )
}

pub fn complete_openai(
    client: &Client,
    api_key: &str,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
) -> Result<StreamResult, String> {
    complete_openai_compatible(client, Some(api_key), url, model, messages)
}

pub fn complete_openai_compatible(
    client: &Client,
    api_key: Option<&str>,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
) -> Result<StreamResult, String> {
    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false
        }));

    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request.send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Provider error: {status} - {body}"));
    }

    let value: Value = response.json().map_err(|e| e.to_string())?;
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

    Ok(StreamResult { content, usage })
}

pub fn complete_anthropic(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
) -> Result<StreamResult, String> {
    let formatted_messages = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| serde_json::json!({ "role": m.role, "content": value_to_string(&m.content) }))
        .collect::<Vec<_>>();

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": model,
            "system": system,
            "messages": formatted_messages,
            "stream": false,
            "max_tokens": 4096
        }))
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Anthropic error: {status} - {body}"));
    }

    let value: Value = response.json().map_err(|e| e.to_string())?;
    let content = value
        .get("content")
        .and_then(|content| content.get(0))
        .and_then(|block| block.get("text"))
        .and_then(|text| text.as_str())
        .unwrap_or("")
        .to_string();

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

    Ok(StreamResult { content, usage })
}

pub fn stream_openai_compatible(
    client: &Client,
    api_key: Option<&str>,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    stream_options: Option<Value>,
    mut on_chunk: impl FnMut(&str),
) -> Result<StreamResult, String> {
    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "stream_options": stream_options,
        }));

    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request.send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Provider error: {status} - {body}"));
    }

    parse_sse_response(response, |value| {
        if let Some(usage) = value.get("usage") {
            let prompt_tokens = usage
                .get("prompt_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            let completion_tokens = usage
                .get("completion_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            if prompt_tokens > 0 || completion_tokens > 0 {
                return Some(Usage {
                    prompt_tokens,
                    completion_tokens,
                });
            }
        }

        if let Some(content) = value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(|content| content.as_str())
        {
            on_chunk(content);
        }

        None
    })
}

pub fn stream_openai_compatible_response(
    response: reqwest::blocking::Response,
    mut on_chunk: impl FnMut(&str),
) -> Result<StreamResult, String> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Provider error: {status} - {body}"));
    }

    parse_sse_response(response, |value| {
        if let Some(usage) = value.get("usage") {
            let prompt_tokens = usage
                .get("prompt_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            let completion_tokens = usage
                .get("completion_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            if prompt_tokens > 0 || completion_tokens > 0 {
                return Some(Usage {
                    prompt_tokens,
                    completion_tokens,
                });
            }
        }

        if let Some(content) = value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(|content| content.as_str())
        {
            on_chunk(content);
        }

        None
    })
}

pub fn stream_anthropic(
    client: &Client,
    api_key: &str,
    model: &str,
    system: Option<&str>,
    messages: &[LlmMessage],
    mut on_chunk: impl FnMut(&str),
) -> Result<StreamResult, String> {
    let formatted_messages = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| serde_json::json!({ "role": m.role, "content": value_to_string(&m.content) }))
        .collect::<Vec<_>>();

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": model,
            "system": system,
            "messages": formatted_messages,
            "stream": true,
            "max_tokens": 4096
        }))
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Anthropic error: {status} - {body}"));
    }

    let mut content = String::new();
    let mut usage: Option<Usage> = None;

    let reader = BufReader::new(response);
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if !trimmed.starts_with("data: ") {
            continue;
        }

        let data = trimmed.trim_start_matches("data: ").trim();
        if data == "[DONE]" {
            break;
        }

        let value: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        if let Some(event_type) = value.get("type").and_then(|v| v.as_str()) {
            if event_type == "content_block_delta" {
                if let Some(text) = value
                    .get("delta")
                    .and_then(|delta| delta.get("text"))
                    .and_then(|text| text.as_str())
                {
                    content.push_str(text);
                    on_chunk(text);
                }
            }

            if event_type == "message_delta" {
                if let Some(delta_usage) = value.get("usage") {
                    let prompt_tokens = delta_usage
                        .get("input_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    let completion_tokens = delta_usage
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    usage = Some(Usage {
                        prompt_tokens,
                        completion_tokens,
                    });
                }
            }
        }
    }

    Ok(StreamResult { content, usage })
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

pub fn stream_ndjson(
    response: reqwest::blocking::Response,
    mut on_chunk: impl FnMut(&str),
) -> Result<StreamResult, String> {
    let mut content = String::new();
    let reader = BufReader::new(response);
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if let Some(text) = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|text| text.as_str())
        {
            content.push_str(text);
            on_chunk(text);
        }
    }

    Ok(StreamResult { content, usage: None })
}

fn parse_sse_response(
    response: reqwest::blocking::Response,
    mut on_value: impl FnMut(Value) -> Option<Usage>,
) -> Result<StreamResult, String> {
    let mut content = String::new();
    let mut usage: Option<Usage> = None;

    let reader = BufReader::new(response);
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if !trimmed.starts_with("data: ") {
            continue;
        }

        let data = trimmed.trim_start_matches("data: ").trim();
        if data == "[DONE]" {
            break;
        }

        let value: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        if let Some(found_usage) = on_value(value.clone()) {
            usage = Some(found_usage);
        }

        if let Some(delta_content) = value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(|content| content.as_str())
        {
            content.push_str(delta_content);
        }
    }

    Ok(StreamResult { content, usage })
}
