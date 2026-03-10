use reqwest::blocking::Client;
use serde_json::Value;

use super::openai::{build_openai_compatible_body, parse_openai_compatible_response};
use super::{compact_error_body, LlmMessage, LlmRequestOptions, StreamResult};

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434/v1/chat/completions";

pub fn complete_ollama_with_output_format_with_options(
    client: &Client,
    url: &str,
    model: &str,
    messages: &[LlmMessage],
    raw_schema: Option<Value>,
    request_options: Option<&LlmRequestOptions>,
) -> Result<StreamResult, String> {
    let mut body = build_openai_compatible_body(model, messages, false, false, request_options);
    if let Some(schema) = raw_schema {
        body["format"] = schema;
    }

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = compact_error_body(response.text().unwrap_or_default());
        return Err(format!("Provider error: {status} - {body}"));
    }

    let value: Value = response.json().map_err(|e| e.to_string())?;
    Ok(parse_openai_compatible_response(&value))
}

// ---------------------------------------------------------------------------
// Provider struct implementing LlmProvider trait
// ---------------------------------------------------------------------------

use super::openai::{
    complete_openai_compatible, stream_openai_compatible_with_options,
};
use super::provider_utils::prepend_system_prompt;
use super::traits::LlmProvider;

/// Ollama provider (local LLM via OpenAI-compatible endpoint).
pub struct OllamaProvider {
    pub model: String,
    pub url: String,
}

impl OllamaProvider {
    pub fn new(model: String, url: Option<String>) -> Self {
        Self {
            model,
            url: url.unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string()),
        }
    }
}

impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
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
        // Ollama uses raw schema in `format` field — NO json_schema envelope.
        complete_ollama_with_output_format_with_options(
            client,
            &self.url,
            &self.model,
            &prepared,
            output_format,
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
            None,
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
        complete_openai_compatible(client, None, &self.url, &self.model, &prepared)
    }

    fn build_request_options(&self, _conversation_id: &str, _phase: &str) -> LlmRequestOptions {
        LlmRequestOptions::default()
    }
}
