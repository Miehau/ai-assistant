use reqwest::blocking::Client;
use serde_json::Value;
use std::io::{BufRead, BufReader};

use super::{compact_error_body, LlmMessage, LlmRequestOptions, StreamResult, Usage};

pub(super) fn build_openai_compatible_body(
    model: &str,
    messages: &[LlmMessage],
    stream: bool,
    include_usage: bool,
    request_options: Option<&LlmRequestOptions>,
) -> Value {
    // TODO(image-support): Map provider-neutral image blocks to OpenAI format
    // before serialising messages. Neutral block: {"type":"image","media_type":"...","data":"..."}
    // OpenAI expects: {"type":"image_url","image_url":{"url":"data:<media_type>;base64,<data>","detail":"auto"}}
    // See docs/image-support-plan.md Phase 2.
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

pub(super) fn parse_openai_compatible_response(value: &Value) -> StreamResult {
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

    StreamResult { content, usage, companion_text: None }
}

fn build_openai_request_body(
    model: &str,
    messages: &[LlmMessage],
    stream: bool,
    include_usage: bool,
    response_format: Option<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<Value, String> {
    let mut body = build_openai_compatible_body(model, messages, stream, include_usage, request_options);
    if let Some(format) = response_format {
        body["response_format"] = format;
    }
    Ok(body)
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

    Ok(StreamResult { content, usage, companion_text: None })
}

// ---------------------------------------------------------------------------
// Provider structs implementing LlmProvider trait
// ---------------------------------------------------------------------------

const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEEPSEEK_URL: &str = "https://api.deepseek.com/chat/completions";
const OPENAI_PROMPT_CACHE_RETENTION: &str = "24h";

fn supports_openai_prompt_cache_retention(model: &str) -> bool {
    let normalized = model.to_ascii_lowercase();
    normalized.starts_with("gpt-5")
}

use super::provider_utils::{prepend_system_prompt, wrap_json_schema_envelope};
use super::traits::LlmProvider;

/// OpenAI provider (native OpenAI API).
pub struct OpenAiProvider {
    pub api_key: String,
    pub model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
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
        let prepared = prepend_system_prompt(messages, system_prompt);
        let response_format = output_format.map(wrap_json_schema_envelope);
        complete_openai_compatible_with_output_format_with_options(
            client,
            Some(&self.api_key),
            OPENAI_URL,
            &self.model,
            &prepared,
            response_format,
            request_options,
        )
    }

    fn stream_response(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        request_options: Option<&LlmRequestOptions>,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<StreamResult, String> {
        let prepared = prepend_system_prompt(messages, system_prompt);
        stream_openai_with_options(
            client,
            &self.api_key,
            OPENAI_URL,
            &self.model,
            &prepared,
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
        let prepared = prepend_system_prompt(messages, system_prompt);
        complete_openai(client, &self.api_key, OPENAI_URL, &self.model, &prepared)
    }

    fn build_request_options(&self, conversation_id: &str, phase: &str) -> LlmRequestOptions {
        let prompt_cache_key = format!("conversation:{conversation_id}:{phase}:v1");
        let prompt_cache_retention = if supports_openai_prompt_cache_retention(&self.model) {
            Some(OPENAI_PROMPT_CACHE_RETENTION.to_string())
        } else {
            None
        };
        LlmRequestOptions {
            prompt_cache_key: Some(prompt_cache_key),
            prompt_cache_retention,
            anthropic_cache_breakpoints: Vec::new(),
        }
    }
}

/// DeepSeek provider (OpenAI-compatible API).
pub struct DeepSeekProvider {
    pub api_key: String,
    pub model: String,
}

impl DeepSeekProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

impl LlmProvider for DeepSeekProvider {
    fn name(&self) -> &str {
        "deepseek"
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
        let prepared = prepend_system_prompt(messages, system_prompt);
        let response_format = output_format.map(wrap_json_schema_envelope);
        complete_openai_compatible_with_output_format_with_options(
            client,
            Some(&self.api_key),
            DEEPSEEK_URL,
            &self.model,
            &prepared,
            response_format,
            request_options,
        )
    }

    fn stream_response(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        request_options: Option<&LlmRequestOptions>,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<StreamResult, String> {
        let prepared = prepend_system_prompt(messages, system_prompt);
        stream_openai_compatible_with_options(
            client,
            Some(&self.api_key),
            DEEPSEEK_URL,
            &self.model,
            &prepared,
            false,
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
        let prepared = prepend_system_prompt(messages, system_prompt);
        complete_openai_compatible(
            client,
            Some(&self.api_key),
            DEEPSEEK_URL,
            &self.model,
            &prepared,
        )
    }

    fn build_request_options(&self, _conversation_id: &str, _phase: &str) -> LlmRequestOptions {
        LlmRequestOptions::default()
    }
}

/// Custom OpenAI-compatible provider with user-specified URL and optional API key.
pub struct CustomProvider {
    pub api_key: Option<String>,
    pub model: String,
    pub url: String,
}

impl CustomProvider {
    pub fn new(api_key: Option<String>, model: String, url: String) -> Self {
        Self {
            api_key,
            model,
            url,
        }
    }
}

impl LlmProvider for CustomProvider {
    fn name(&self) -> &str {
        "custom"
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
        let prepared = prepend_system_prompt(messages, system_prompt);
        let response_format = output_format.map(wrap_json_schema_envelope);
        complete_openai_compatible_with_output_format_with_options(
            client,
            self.api_key.as_deref(),
            &self.url,
            &self.model,
            &prepared,
            response_format,
            request_options,
        )
    }

    fn stream_response(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        request_options: Option<&LlmRequestOptions>,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<StreamResult, String> {
        let prepared = prepend_system_prompt(messages, system_prompt);
        stream_openai_compatible_with_options(
            client,
            self.api_key.as_deref(),
            &self.url,
            &self.model,
            &prepared,
            false,
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
        let prepared = prepend_system_prompt(messages, system_prompt);
        complete_openai_compatible(
            client,
            self.api_key.as_deref(),
            &self.url,
            &self.model,
            &prepared,
        )
    }

    fn build_request_options(&self, _conversation_id: &str, _phase: &str) -> LlmRequestOptions {
        LlmRequestOptions::default()
    }
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
    fn openai_request_body_passes_through_response_format() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("hello"),
        }];
        let format = json!({
            "type": "json_schema",
            "json_schema": { "name": "response", "strict": false, "schema": { "type": "object" } }
        });
        let body = build_openai_request_body(
            "gpt-5-mini",
            &messages,
            false,
            false,
            Some(format.clone()),
            None,
        )
        .expect("body");
        assert_eq!(body.get("response_format"), Some(&format));
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
}
