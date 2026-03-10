use reqwest::blocking::Client;
use serde_json::Value;

use super::{LlmMessage, LlmRequestOptions, StreamResult};

/// Unified interface for LLM provider operations.
/// All provider-specific behavior (URL, auth, schema wrapping,
/// system prompt handling) is encapsulated in implementations.
pub trait LlmProvider: Send + Sync {
    /// Human-readable provider name (e.g., "openai", "anthropic")
    fn name(&self) -> &str;

    /// Whether this provider supports streaming responses.
    fn supports_streaming(&self) -> bool;

    /// Controller call: structured output or tool-use, depending on provider.
    ///
    /// - `messages`: conversation messages (without system prompt prepended)
    /// - `system_prompt`: optional system prompt (provider decides how to inject)
    /// - `output_format`: optional JSON schema for structured output
    /// - `request_options`: caching and other per-request config
    ///
    /// Returns the raw LLM response content + usage.
    fn controller_call(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        output_format: Option<Value>,
        request_options: Option<&LlmRequestOptions>,
    ) -> Result<StreamResult, String>;

    /// Streaming response (responder phase).
    /// Default implementation returns an error for non-streaming providers.
    fn stream_response(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        request_options: Option<&LlmRequestOptions>,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<StreamResult, String> {
        let _ = (client, messages, system_prompt, request_options, on_chunk);
        Err(format!(
            "Provider '{}' does not support streaming",
            self.name()
        ))
    }

    /// Simple completion (used for title generation, triage, etc.)
    /// No structured output, no streaming.
    fn complete(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
    ) -> Result<StreamResult, String>;

    /// Build request options appropriate for this provider.
    fn build_request_options(&self, conversation_id: &str, phase: &str) -> LlmRequestOptions;
}

/// Everything needed to construct a provider instance.
/// Resolved once at the start of a request, then passed around.
pub struct ProviderConfig {
    pub provider_name: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

use super::anthropic::AnthropicProvider;
use super::claude_cli::ClaudeCliProvider;
use super::ollama::OllamaProvider;
use super::openai::{CustomProvider, DeepSeekProvider, OpenAiProvider};

/// Construct the appropriate LlmProvider from config.
/// Validates that required fields (api_key, url) are present.
pub fn create_provider(config: ProviderConfig) -> Result<Box<dyn LlmProvider>, String> {
    match config.provider_name.as_str() {
        "openai" => {
            let api_key = config
                .api_key
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "Missing API key for provider: openai".to_string())?;
            Ok(Box::new(OpenAiProvider::new(api_key, config.model)))
        }
        "anthropic" => {
            let api_key = config
                .api_key
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "Missing API key for provider: anthropic".to_string())?;
            Ok(Box::new(AnthropicProvider::new(api_key, config.model)))
        }
        "deepseek" => {
            let api_key = config
                .api_key
                .filter(|k| !k.is_empty())
                .ok_or_else(|| "Missing API key for provider: deepseek".to_string())?;
            Ok(Box::new(DeepSeekProvider::new(api_key, config.model)))
        }
        "ollama" => Ok(Box::new(OllamaProvider::new(config.model, config.base_url))),
        "claude_cli" => Ok(Box::new(ClaudeCliProvider::new(config.model))),
        "custom" => {
            let url = config
                .base_url
                .filter(|u| !u.is_empty())
                .ok_or_else(|| "Custom provider requires a base URL".to_string())?;
            Ok(Box::new(CustomProvider::new(
                config.api_key,
                config.model,
                url,
            )))
        }
        other => Err(format!("Unsupported provider: {other}")),
    }
}
