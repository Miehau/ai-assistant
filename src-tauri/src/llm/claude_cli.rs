use serde_json::Value;
use std::process::Command;

use super::{value_to_string, LlmMessage, StreamResult};

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
        // TODO(image-support): Claude CLI is text-only. When content is a
        // provider-neutral block array, extract text blocks and log a warning
        // for any image blocks that are silently dropped.
        // See docs/image-support-plan.md Phase 2.
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
        companion_text: None,
    })
}

// ---------------------------------------------------------------------------
// Provider struct implementing LlmProvider trait
// ---------------------------------------------------------------------------

use reqwest::blocking::Client;
use super::LlmRequestOptions;
use super::traits::LlmProvider;

/// Claude CLI provider (uses `claude` subprocess, not HTTP).
pub struct ClaudeCliProvider {
    pub model: String,
}

impl ClaudeCliProvider {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

impl LlmProvider for ClaudeCliProvider {
    fn name(&self) -> &str {
        "claude_cli"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn controller_call(
        &self,
        _client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        output_format: Option<Value>,
        _request_options: Option<&LlmRequestOptions>,
    ) -> Result<StreamResult, String> {
        // claude_cli passes system prompt separately (handled inside format_claude_cli_prompt).
        complete_claude_cli(&self.model, system_prompt, messages, output_format)
    }

    // stream_response: uses default trait impl (returns error)

    fn complete(
        &self,
        _client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
    ) -> Result<StreamResult, String> {
        complete_claude_cli(&self.model, system_prompt, messages, None)
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
    fn normalize_claude_cli_model_strips_prefix() {
        assert_eq!(
            normalize_claude_cli_model("claude-cli-3-5-sonnet"),
            "claude-3-5-sonnet"
        );
    }

    #[test]
    fn normalize_claude_cli_model_passes_through_other() {
        assert_eq!(
            normalize_claude_cli_model("claude-3-5-sonnet"),
            "claude-3-5-sonnet"
        );
    }

    #[test]
    fn format_claude_cli_prompt_includes_system_and_messages() {
        let messages = vec![
            LlmMessage {
                role: "user".to_string(),
                content: json!("hello"),
            },
        ];
        let prompt = format_claude_cli_prompt(&messages, Some("be helpful"), None);
        assert!(prompt.contains("System:\nbe helpful"));
        assert!(prompt.contains("User:\nhello"));
        assert!(prompt.ends_with("Assistant:\n"));
    }

    #[test]
    fn format_claude_cli_prompt_includes_schema_instructions() {
        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!("test"),
        }];
        let format = json!({
            "type": "json_schema",
            "schema": { "type": "object", "properties": { "action": { "type": "string" } } }
        });
        let prompt = format_claude_cli_prompt(&messages, None, Some(&format));
        assert!(prompt.contains("Return ONLY valid JSON"));
        assert!(prompt.contains("\"action\""));
    }
}
