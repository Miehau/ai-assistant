use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;

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
    println!(
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

    println!(
        "[llm] provider=openai_compatible model={} content_len={} usage={:?}",
        model,
        content.len(),
        usage.as_ref().map(|u| (u.prompt_tokens, u.completion_tokens))
    );

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

    let output_format = serde_json::json!({
        "type": "json_schema",
        "schema": {
            "type": "object",
            "properties": {
                "type": { "type": "string", "enum": ["direct_response", "plan", "tool_calls"] },
                "content": { "type": "string" },
                "steps": { "type": "array", "items": { "type": "string" } },
                "calls": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "args": { "type": "object" }
                        },
                        "required": ["tool", "args"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["type"],
            "additionalProperties": false
        }
    });

    let body = serde_json::json!({
        "model": model,
        "system": system,
        "messages": formatted_messages,
        "stream": false,
        "max_tokens": 4096,
        "temperature": 0,
        "output_format": output_format
    });

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
            let body = response.text().unwrap_or_default();
            return Err(format!("Anthropic error: {status} - {body}"));
        }

        response.json().map_err(|e| e.to_string())
    };

    let value = send_request(&body, true)?;

    println!(
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

    println!(
        "[llm] provider=anthropic model={} content_len={} usage={:?}",
        model,
        content.len(),
        usage.as_ref().map(|u| (u.prompt_tokens, u.completion_tokens))
    );

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
