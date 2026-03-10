use serde_json::Value;

use super::LlmMessage;

/// Prepend a system prompt as the first message (OpenAI-compatible style).
/// Anthropic/claude_cli handle system prompts differently and should NOT use this.
pub fn prepend_system_prompt(
    messages: &[LlmMessage],
    system_prompt: Option<&str>,
) -> Vec<LlmMessage> {
    let mut prepared = messages.to_vec();
    if let Some(sp) = system_prompt {
        if !sp.trim().is_empty() {
            prepared.insert(
                0,
                LlmMessage {
                    role: "system".to_string(),
                    content: serde_json::json!(sp),
                },
            );
        }
    }
    prepared
}

/// Wrap a raw JSON schema in the OpenAI-style json_schema response_format envelope.
pub fn wrap_json_schema_envelope(schema: Value) -> Value {
    serde_json::json!({
        "type": "json_schema",
        "json_schema": { "name": "response", "strict": false, "schema": schema }
    })
}
