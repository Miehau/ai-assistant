use serde_json::{json, Value};

use crate::tool_outputs::{store_tool_output, ToolOutputRecord};
use crate::tools::ToolResultMode;

use super::controller_parsing::{OutputDeliveryResolution, OutputModeHint, ResolvedOutputMode};
use super::output_metadata::compute_output_metadata_with_size;
use super::text_utils::{summarize_tool_output_value, truncate_with_notice, value_char_len};

const AUTO_INLINE_RESULT_MAX_CHARS: usize = 32_768;
const INLINE_RESULT_HARD_MAX_CHARS: usize = 65_536;
pub const PERSISTED_RESULT_PREVIEW_MAX_CHARS: usize = 1_200;

const TOOL_OUTPUTS_PREFIX: &str = "tool_outputs.";

const AVAILABLE_TOOLS_HINT: [&str; 6] = [
    "tool_outputs.read — load full output into context",
    "tool_outputs.extract — extract fields via JSONPath",
    "tool_outputs.stats — get schema, field types, counts",
    "tool_outputs.count — count items matching criteria",
    "tool_outputs.sample — sample items from arrays",
    "tool_outputs.list — list all stored outputs",
];

pub struct ToolOutputDeliveryResult {
    pub success: bool,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub delivery: OutputDeliveryResolution,
    pub artifact_persist_warning: Option<String>,
}

/// Unified output delivery: resolves output mode, persists artifact, builds response.
/// Called by both single-tool and parallel-tool execution paths.
pub fn build_tool_output_delivery(
    execution_id: &str,
    tool_name: &str,
    conversation_id: &str,
    message_id: &str,
    args: &Value,
    output_value: Value,
    requested_output_mode: OutputModeHint,
    result_mode: &ToolResultMode,
) -> ToolOutputDeliveryResult {
    let output_chars = value_char_len(&output_value);
    let delivery = resolve_output_delivery(
        tool_name,
        requested_output_mode,
        result_mode,
        output_chars,
    );

    let (preview, _preview_truncated) =
        summarize_tool_output_value(&output_value, PERSISTED_RESULT_PREVIEW_MAX_CHARS);
    let metadata = compute_output_metadata_with_size(&output_value, Some(output_chars));
    let should_store_artifact = !tool_name.starts_with(TOOL_OUTPUTS_PREFIX);

    let (output_ref, persist_error) = if should_store_artifact {
        let record = ToolOutputRecord {
            id: execution_id.to_string(),
            tool_name: tool_name.to_string(),
            conversation_id: Some(conversation_id.to_string()),
            message_id: message_id.to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            success: true,
            parameters: args.clone(),
            output: output_value.clone(),
        };
        match store_tool_output(&record) {
            Ok(output_ref) => (Some(output_ref), None),
            Err(err) => (None, Some(format!("Failed to persist tool output: {err}"))),
        }
    } else {
        (None, None)
    };

    match delivery.resolved_output_mode {
        ResolvedOutputMode::Inline => {
            let artifact_persist_warning = persist_error.map(|error_message| {
                log::warn!(
                    "[tool] artifact persistence warning: tool={} execution_id={} warning={}",
                    tool_name,
                    execution_id,
                    error_message
                );
                error_message
            });
            ToolOutputDeliveryResult {
                success: true,
                output: Some(output_value),
                error: None,
                delivery,
                artifact_persist_warning,
            }
        }
        ResolvedOutputMode::Persist => {
            if let Some(error_message) = persist_error {
                ToolOutputDeliveryResult {
                    success: false,
                    output: Some(json!({
                        "message": error_message,
                        "success": false
                    })),
                    error: Some(error_message),
                    delivery,
                    artifact_persist_warning: None,
                }
            } else if let Some(output_ref) = output_ref {
                let smart_preview = build_smart_preview(
                    tool_name,
                    &output_value,
                    &preview,
                    PERSISTED_RESULT_PREVIEW_MAX_CHARS,
                );
                let mut envelope = json!({
                    "persisted": true,
                    "output_ref": output_ref,
                    "size_chars": output_chars as i64,
                    "preview": smart_preview,
                    "metadata": metadata,
                    "available_tools": AVAILABLE_TOOLS_HINT
                });
                // Carry over content_blocks (e.g. image blocks from files.read) so
                // the orchestrator can lift them into the LLM message even when the
                // bulk text/base64 data is persisted to disk.
                if let Some(blocks) = output_value.get("content_blocks").cloned() {
                    envelope["content_blocks"] = blocks;
                }
                ToolOutputDeliveryResult {
                    success: true,
                    output: Some(envelope),
                    error: None,
                    delivery,
                    artifact_persist_warning: None,
                }
            } else {
                let error_message =
                    "Resolved persisted output but missing output_ref".to_string();
                ToolOutputDeliveryResult {
                    success: false,
                    output: Some(json!({
                        "message": error_message,
                        "success": false
                    })),
                    error: Some(error_message),
                    delivery,
                    artifact_persist_warning: None,
                }
            }
        }
    }
}

pub fn resolve_output_delivery(
    tool_name: &str,
    requested_output_mode: OutputModeHint,
    result_mode: &ToolResultMode,
    output_chars: usize,
) -> OutputDeliveryResolution {
    if tool_name.starts_with(TOOL_OUTPUTS_PREFIX) {
        return OutputDeliveryResolution {
            requested_output_mode,
            resolved_output_mode: ResolvedOutputMode::Inline,
            forced_persist: false,
            forced_reason: None,
        };
    }

    match requested_output_mode {
        OutputModeHint::Persist => OutputDeliveryResolution {
            requested_output_mode,
            resolved_output_mode: ResolvedOutputMode::Persist,
            forced_persist: false,
            forced_reason: None,
        },
        OutputModeHint::Inline => {
            if output_chars > INLINE_RESULT_HARD_MAX_CHARS {
                OutputDeliveryResolution {
                    requested_output_mode,
                    resolved_output_mode: ResolvedOutputMode::Persist,
                    forced_persist: true,
                    forced_reason: Some("inline_size_exceeds_hard_limit"),
                }
            } else {
                OutputDeliveryResolution {
                    requested_output_mode,
                    resolved_output_mode: ResolvedOutputMode::Inline,
                    forced_persist: false,
                    forced_reason: None,
                }
            }
        }
        OutputModeHint::Auto => {
            let should_persist =
                should_persist_tool_output(tool_name, result_mode, output_chars);
            let forced_persist =
                matches!(result_mode, ToolResultMode::Inline) && should_persist;
            OutputDeliveryResolution {
                requested_output_mode,
                resolved_output_mode: if should_persist {
                    ResolvedOutputMode::Persist
                } else {
                    ResolvedOutputMode::Inline
                },
                forced_persist,
                forced_reason: if forced_persist {
                    Some("inline_size_exceeds_hard_limit")
                } else {
                    None
                },
            }
        }
    }
}

fn should_persist_tool_output(
    tool_name: &str,
    result_mode: &ToolResultMode,
    output_chars: usize,
) -> bool {
    if tool_name.starts_with(TOOL_OUTPUTS_PREFIX) {
        return false;
    }

    match result_mode {
        ToolResultMode::Inline => output_chars > INLINE_RESULT_HARD_MAX_CHARS,
        ToolResultMode::Persist => true,
        ToolResultMode::Auto => output_chars > AUTO_INLINE_RESULT_MAX_CHARS,
    }
}

/// Build a structure-aware preview for persisted output.
/// For known tool types (gmail, calendar), extracts meaningful fields.
/// Falls back to raw JSON truncation for unknown tools.
fn build_smart_preview(
    tool_name: &str,
    output_value: &Value,
    raw_preview: &str,
    max_chars: usize,
) -> String {
    if let Some(preview) = try_gmail_smart_preview(tool_name, output_value, max_chars) {
        return preview;
    }
    if let Some(preview) = try_structured_preview(output_value, max_chars) {
        return preview;
    }
    raw_preview.to_string()
}

fn try_gmail_smart_preview(tool_name: &str, value: &Value, max_chars: usize) -> Option<String> {
    if !tool_name.starts_with("gmail.") {
        return None;
    }

    match tool_name {
        "gmail.list_threads" => {
            let threads = value.get("threads")?.as_array()?;
            let mut lines = Vec::new();
            lines.push(format!("{} threads found", threads.len()));
            for (i, thread) in threads.iter().take(10).enumerate() {
                let id = thread.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let snippet = thread
                    .get("snippet")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let snippet_short: String = snippet.chars().take(80).collect();
                lines.push(format!("  [{}] id={} \"{}\"", i + 1, id, snippet_short));
            }
            if threads.len() > 10 {
                lines.push(format!("  ... and {} more", threads.len() - 10));
            }
            if let Some(token) = value.get("nextPageToken").and_then(|v| v.as_str()) {
                lines.push(format!("  nextPageToken: {token}"));
            }
            let result = lines.join("\n");
            Some(truncate_with_notice(&result, max_chars))
        }
        "gmail.get_thread" => {
            let thread_id = value
                .get("thread_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let mode = value
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let mut lines = Vec::new();
            lines.push(format!("thread_id={thread_id} mode={mode}"));

            if let Some(msg) = value.get("message") {
                append_message_preview(&mut lines, msg, max_chars / 2);
            }
            if let Some(messages) = value.get("messages").and_then(|v| v.as_array()) {
                lines.push(format!("{} messages:", messages.len()));
                for msg in messages.iter().take(5) {
                    append_message_preview(&mut lines, msg, max_chars / 6);
                }
                if messages.len() > 5 {
                    lines.push(format!("  ... and {} more messages", messages.len() - 5));
                }
            }

            let result = lines.join("\n");
            Some(truncate_with_notice(&result, max_chars))
        }
        _ => None,
    }
}

fn append_message_preview(lines: &mut Vec<String>, msg: &Value, body_limit: usize) {
    let title = msg.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let date = msg.get("date_header").and_then(|v| v.as_str()).unwrap_or("");
    let msg_id = msg
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    lines.push(format!("  message_id={msg_id}"));
    if !title.is_empty() {
        lines.push(format!("  subject: {title}"));
    }
    if !date.is_empty() {
        lines.push(format!("  date: {date}"));
    }
    if let Some(body) = msg.get("body_text").and_then(|v| v.as_str()) {
        let snippet: String = body.chars().take(body_limit).collect();
        lines.push(format!("  body: {snippet}"));
    }
    if let Some(attachments) = msg.get("attachments").and_then(|v| v.as_array()) {
        if !attachments.is_empty() {
            let names: Vec<&str> = attachments
                .iter()
                .filter_map(|a| a.get("filename").and_then(|v| v.as_str()))
                .take(5)
                .collect();
            lines.push(format!("  attachments: {}", names.join(", ")));
        }
    }
}

/// For non-gmail structured output, extract top-level keys and short values.
fn try_structured_preview(value: &Value, max_chars: usize) -> Option<String> {
    let map = value.as_object()?;
    if map.len() > 20 {
        return None; // Too complex for structured preview
    }

    let mut lines = Vec::new();
    let mut total = 0;
    for (key, val) in map.iter() {
        let line = match val {
            Value::String(s) => {
                let short: String = s.chars().take(120).collect();
                let ellipsis = if s.len() > 120 { "..." } else { "" };
                format!("{key}: \"{short}{ellipsis}\"")
            }
            Value::Number(n) => format!("{key}: {n}"),
            Value::Bool(b) => format!("{key}: {b}"),
            Value::Null => format!("{key}: null"),
            Value::Array(arr) => format!("{key}: [{} items]", arr.len()),
            Value::Object(inner) => format!("{key}: {{}} ({} keys)", inner.len()),
        };
        total += line.len() + 1;
        if total > max_chars {
            lines.push("...".to_string());
            break;
        }
        lines.push(line);
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_persist_skips_all_tool_outputs_tools() {
        let tool_outputs_tools = [
            "tool_outputs.read",
            "tool_outputs.list",
            "tool_outputs.stats",
            "tool_outputs.extract",
            "tool_outputs.count",
            "tool_outputs.sample",
        ];
        for tool_name in &tool_outputs_tools {
            let delivery = resolve_output_delivery(
                tool_name,
                OutputModeHint::Auto,
                &ToolResultMode::Auto,
                100_000,
            );
            assert_eq!(
                delivery.resolved_output_mode,
                ResolvedOutputMode::Inline,
                "tool_outputs tool '{tool_name}' should always resolve to inline"
            );
        }
    }

    #[test]
    fn resolve_output_delivery_inline_small_stays_inline() {
        let delivery = resolve_output_delivery(
            "some.tool",
            OutputModeHint::Inline,
            &ToolResultMode::Auto,
            100,
        );
        assert_eq!(delivery.resolved_output_mode, ResolvedOutputMode::Inline);
        assert!(!delivery.forced_persist);
    }

    #[test]
    fn resolve_output_delivery_inline_large_forces_persist() {
        let delivery = resolve_output_delivery(
            "some.tool",
            OutputModeHint::Inline,
            &ToolResultMode::Auto,
            65_536 + 1,
        );
        assert_eq!(delivery.resolved_output_mode, ResolvedOutputMode::Persist);
        assert!(delivery.forced_persist);
        assert_eq!(
            delivery.forced_reason,
            Some("inline_size_exceeds_hard_limit")
        );
    }

    #[test]
    fn resolve_output_delivery_persist_requested_persists() {
        let delivery = resolve_output_delivery(
            "some.tool",
            OutputModeHint::Persist,
            &ToolResultMode::Auto,
            100,
        );
        assert_eq!(delivery.resolved_output_mode, ResolvedOutputMode::Persist);
        assert!(!delivery.forced_persist);
    }

    #[test]
    fn resolve_output_delivery_auto_follows_tool_result_mode() {
        // Auto + ToolResultMode::Persist => persist
        let delivery = resolve_output_delivery(
            "some.tool",
            OutputModeHint::Auto,
            &ToolResultMode::Persist,
            100,
        );
        assert_eq!(delivery.resolved_output_mode, ResolvedOutputMode::Persist);

        // Auto + ToolResultMode::Inline + small => inline
        let delivery = resolve_output_delivery(
            "some.tool",
            OutputModeHint::Auto,
            &ToolResultMode::Inline,
            100,
        );
        assert_eq!(delivery.resolved_output_mode, ResolvedOutputMode::Inline);
    }

    #[test]
    fn resolve_output_delivery_tool_outputs_stays_inline() {
        let delivery = resolve_output_delivery(
            "tool_outputs.read",
            OutputModeHint::Persist,
            &ToolResultMode::Persist,
            100_000,
        );
        assert_eq!(delivery.resolved_output_mode, ResolvedOutputMode::Inline);
        assert!(!delivery.forced_persist);
    }
}
