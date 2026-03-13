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

#[derive(Debug)]
pub struct StreamResult {
    pub content: String,
    pub usage: Option<Usage>,
    /// Text blocks emitted alongside tool_use blocks (Anthropic only).
    /// Surfaced to the user and appended to conversation context.
    pub companion_text: Option<String>,
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

/// A provider-neutral content block, parsed from `LlmMessage.content`.
/// Content may be a plain string or a JSON array of `{type, ...}` blocks.
enum ContentBlock {
    Text(String),
    Image { media_type: String, data: String },
}

/// Parse `LlmMessage.content` into typed blocks.
/// Plain strings become a single `Text` block.
/// Unknown block types are silently skipped.
fn parse_content_blocks(content: &Value) -> Vec<ContentBlock> {
    if let Some(text) = content.as_str() {
        return vec![ContentBlock::Text(text.to_string())];
    }

    if let Some(blocks) = content.as_array() {
        let mut result = Vec::new();
        for block in blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    let text = block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    result.push(ContentBlock::Text(text));
                }
                Some("image") => {
                    let media_type = block
                        .get("media_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("image/jpeg")
                        .to_string();
                    let data = block
                        .get("data")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    result.push(ContentBlock::Image { media_type, data });
                }
                _ => {}
            }
        }
        return result;
    }

    // Fallback for unexpected Value shapes (e.g. numbers, objects)
    vec![ContentBlock::Text(value_to_string(content))]
}
