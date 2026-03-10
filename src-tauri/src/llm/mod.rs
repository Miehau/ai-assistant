use serde::Serialize;
use serde_json::Value;

mod anthropic;
mod claude_cli;
mod ollama;
mod openai;
mod provider_utils;
mod traits;

// Re-export trait-based provider infrastructure
pub use traits::{create_provider, ProviderConfig};

// Re-export pub(crate) functions (test-only — schema builder is only needed for existing tests)
#[cfg(test)]
pub(crate) use anthropic::build_anthropic_output_schema;
#[cfg(test)]
pub(crate) use anthropic::validate_anthropic_output_format;

// Re-export public functions still used by other modules (e.g. subagent.rs).
// Functions only used internally by provider structs are NOT re-exported here.
pub use anthropic::complete_anthropic_with_output_format_with_options;
pub use claude_cli::complete_claude_cli;
pub use openai::{
    complete_openai_compatible_with_options, complete_openai_with_options,
};

const PROVIDER_ERROR_BODY_MAX_CHARS: usize = 2_000;

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
