use chrono::Utc;
use serde_json::{json, Value};
use std::time::Instant;

use crate::db::{StepResult, ToolExecutionRecord};

use super::controller_parsing::{OutputDeliveryResolution, OutputModeHint};
use super::output_delivery::PERSISTED_RESULT_PREVIEW_MAX_CHARS;
use super::output_metadata::{compute_output_metadata, strip_metadata_id_hints};
use super::text_utils::{summarize_tool_args, summarize_tool_output_value, truncate_with_notice};
use super::tool_arg_hydration::extract_tool_output_ref_id_from_value;

const CONTROLLER_TOOL_SUMMARY_MAX_CHARS: usize = 2_000;
const CONTROLLER_TOOL_SUMMARY_MAX_ARGS_CHARS: usize = 400;
const CONTROLLER_TOOL_SUMMARY_MAX_RESULT_CHARS: usize = 800;
const CONTROLLER_TOOL_SUMMARY_MAX_METADATA_CHARS: usize = 320;

pub fn build_tool_execution_record(
    execution_id: String,
    tool_name: String,
    args: Value,
    result: Option<Value>,
    success: bool,
    error: Option<String>,
    duration_ms: i64,
    iteration: usize,
    timestamp_ms: i64,
    requested_output_mode: Option<OutputModeHint>,
    output_delivery: Option<&OutputDeliveryResolution>,
) -> ToolExecutionRecord {
    ToolExecutionRecord {
        execution_id,
        tool_name,
        args,
        result,
        success,
        error,
        duration_ms,
        iteration,
        timestamp_ms,
        requested_output_mode: requested_output_mode.map(|m| m.as_str().to_string()),
        resolved_output_mode: output_delivery
            .map(|d| d.resolved_output_mode.as_str().to_string()),
        forced_persist: output_delivery.map(|d| d.forced_persist),
        forced_reason: output_delivery.and_then(|d| d.forced_reason.map(str::to_string)),
    }
}

pub fn build_batch_step_result(
    step_id: &str,
    started: Instant,
    execution_mode: &str,
    requested_calls: usize,
    dropped_calls: usize,
    successful_calls: usize,
    results_summary: Vec<Value>,
    aggregated_tool_executions: Vec<ToolExecutionRecord>,
    first_error: Option<String>,
) -> StepResult {
    let duration_ms = started.elapsed().as_millis() as i64;
    let total_calls = results_summary.len();
    let success = first_error.is_none();
    let output = Some(json!({
        "success": success,
        "batch_size": total_calls,
        "requested_calls": requested_calls,
        "executed_calls": total_calls,
        "dropped_calls": dropped_calls,
        "successful_calls": successful_calls,
        "failed_calls": total_calls.saturating_sub(successful_calls),
        "execution_mode": execution_mode,
        "results": results_summary
    }));

    StepResult {
        step_id: step_id.to_string(),
        success,
        output,
        error: first_error,
        tool_executions: aggregated_tool_executions,
        duration_ms,
        completed_at: Utc::now(),
    }
}

pub fn build_tool_batch_result_summary(execution: &ToolExecutionRecord) -> Value {
    let output_ref = execution
        .result
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
        .unwrap_or_else(|| "none".to_string());
    let metadata = execution
        .result
        .as_ref()
        .and_then(|value| value.get("metadata"))
        .cloned()
        .unwrap_or(Value::Null);
    let preview = if execution.success {
        execution
            .result
            .as_ref()
            .and_then(|value| value.get("preview"))
            .and_then(|value| value.as_str())
            .map(|value| truncate_with_notice(value, PERSISTED_RESULT_PREVIEW_MAX_CHARS))
            .unwrap_or_else(|| {
                execution
                    .result
                    .as_ref()
                    .map(|value| {
                        summarize_tool_output_value(value, PERSISTED_RESULT_PREVIEW_MAX_CHARS).0
                    })
                    .unwrap_or_else(|| "none".to_string())
            })
    } else {
        execution
            .error
            .clone()
            .unwrap_or_else(|| "Tool execution failed".to_string())
    };

    json!({
        "tool": execution.tool_name,
        "execution_id": execution.execution_id,
        "success": execution.success,
        "requested_output_mode": execution.requested_output_mode,
        "resolved_output_mode": execution.resolved_output_mode,
        "forced_persist": execution.forced_persist,
        "forced_reason": execution.forced_reason,
        "output_ref": output_ref,
        "metadata": metadata,
        "preview": preview,
        "error": execution.error
    })
}

pub fn format_tool_execution_summary_block(exec: &ToolExecutionRecord) -> String {
    let args = summarize_tool_args(&exec.args, CONTROLLER_TOOL_SUMMARY_MAX_ARGS_CHARS);
    let output_ref = exec
        .result
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
        .unwrap_or_else(|| "none".to_string());
    let requested_output_mode = exec
        .requested_output_mode
        .clone()
        .or_else(|| {
            exec.result
                .as_ref()
                .and_then(|value| value.get("requested_output_mode"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "n/a".to_string());
    let resolved_output_mode = exec
        .resolved_output_mode
        .clone()
        .or_else(|| {
            exec.result
                .as_ref()
                .and_then(|value| value.get("resolved_output_mode"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            if output_ref != "none" {
                "persist".to_string()
            } else if exec.success {
                "inline".to_string()
            } else {
                "n/a".to_string()
            }
        });
    let forced_persist = exec.forced_persist.or_else(|| {
        exec.result
            .as_ref()
            .and_then(|value| value.get("forced_persist"))
            .and_then(|value| value.as_bool())
    });
    let forced_reason = exec
        .forced_reason
        .clone()
        .or_else(|| {
            exec.result
                .as_ref()
                .and_then(|value| value.get("forced_reason"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "none".to_string());

    let is_persist = resolved_output_mode == "persist";
    let metadata_value = if let Some(value) = exec
        .result
        .as_ref()
        .and_then(|value| value.get("metadata"))
        .filter(|value| !value.is_null())
    {
        value.clone()
    } else if let Some(value) = exec.result.as_ref() {
        if is_persist {
            Value::Null
        } else {
            compute_output_metadata(value)
        }
    } else {
        Value::Null
    };
    let metadata_value = if is_persist {
        strip_metadata_id_hints(&metadata_value)
    } else {
        metadata_value
    };
    let metadata_summary = if metadata_value.is_null() {
        "none".to_string()
    } else {
        truncate_with_notice(
            &metadata_value.to_string(),
            CONTROLLER_TOOL_SUMMARY_MAX_METADATA_CHARS,
        )
    };

    let mut summary = format!(
        "Tool: {} | ExecutionId: {} | Success: {} | RequestedOutputMode: {} | ResolvedOutputMode: {} | ForcedPersist: {} | ForcedReason: {} | OutputRef: {} | Args: {} | Metadata: {}",
        exec.tool_name,
        exec.execution_id,
        exec.success,
        requested_output_mode,
        resolved_output_mode,
        forced_persist.unwrap_or(false),
        forced_reason,
        output_ref,
        args,
        metadata_summary
    );

    if !exec.success {
        let error = exec.error.as_deref().unwrap_or("Tool execution failed");
        summary.push_str(" | Error: ");
        summary.push_str(&truncate_with_notice(
            error,
            CONTROLLER_TOOL_SUMMARY_MAX_RESULT_CHARS,
        ));
        return summary;
    }

    if is_persist {
        summary.push_str(
            " | Note: Exact values require tool_outputs.extract (omit id to hydrate latest output_ref).",
        );
        return summary;
    }

    let output_json = exec
        .result
        .as_ref()
        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| value.to_string()))
        .unwrap_or_else(|| "none".to_string());
    summary.push_str(" | Output: ");
    summary.push_str(&output_json);
    summary
}

pub fn format_tool_execution_batch_summary_line(exec: &ToolExecutionRecord) -> String {
    let output_ref = exec
        .result
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
        .unwrap_or_else(|| "none".to_string());
    let error = if exec.success {
        "none".to_string()
    } else {
        truncate_with_notice(
            exec.error.as_deref().unwrap_or("Tool execution failed"),
            CONTROLLER_TOOL_SUMMARY_MAX_RESULT_CHARS / 4,
        )
    };

    format!(
        "Tool: {} | ExecutionId: {} | Success: {} | OutputRef: {} | Error: {}",
        exec.tool_name, exec.execution_id, exec.success, output_ref, error
    )
}

pub const TOOL_SUMMARY_MAX_CHARS: usize = CONTROLLER_TOOL_SUMMARY_MAX_CHARS;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_execution_record(
        tool_name: &str,
        execution_id: &str,
        result: Option<Value>,
        success: bool,
    ) -> ToolExecutionRecord {
        ToolExecutionRecord {
            execution_id: execution_id.to_string(),
            tool_name: tool_name.to_string(),
            args: json!({}),
            result,
            success,
            error: if success {
                None
            } else {
                Some("failed".to_string())
            },
            duration_ms: 100,
            iteration: 0,
            timestamp_ms: 1000,
            requested_output_mode: Some("persist".to_string()),
            resolved_output_mode: Some("persist".to_string()),
            forced_persist: Some(false),
            forced_reason: None,
        }
    }

    #[test]
    fn format_tool_execution_summary_block_includes_envelope_fields() {
        let exec = make_execution_record(
            "gmail.list_threads",
            "exec-001",
            Some(json!({
                "persisted": true,
                "output_ref": { "id": "exec-001" },
                "metadata": { "root_type": "object", "size_chars": 5000 }
            })),
            true,
        );
        let summary = format_tool_execution_summary_block(&exec);
        assert!(summary.contains("gmail.list_threads"), "should contain tool name");
        assert!(summary.contains("exec-001"), "should contain execution id");
        assert!(summary.contains("OutputRef: exec-001"), "should contain output ref");
        assert!(summary.contains("persist"), "should contain output mode");
    }

    #[test]
    fn format_tool_execution_summary_block_includes_full_inline_payload() {
        let mut exec = make_execution_record(
            "test.echo",
            "exec-002",
            Some(json!({ "text": "hello" })),
            true,
        );
        exec.requested_output_mode = Some("inline".to_string());
        exec.resolved_output_mode = Some("inline".to_string());
        let summary = format_tool_execution_summary_block(&exec);
        assert!(summary.contains("Output:"), "inline result should have Output field");
        assert!(summary.contains("hello"), "should contain inline output value");
    }

    #[test]
    fn format_tool_execution_batch_summary_line_keeps_identity_fields() {
        let exec = make_execution_record(
            "gmail.get_thread",
            "exec-003",
            Some(json!({
                "output_ref": { "id": "exec-003" }
            })),
            true,
        );
        let line = format_tool_execution_batch_summary_line(&exec);
        assert!(line.contains("gmail.get_thread"), "should contain tool name");
        assert!(line.contains("exec-003"), "should contain execution id");
        assert!(line.contains("OutputRef: exec-003"), "should contain output ref");
    }
}
