use serde_json::{json, Value};

use crate::db::StepResult;
use crate::tool_outputs::tool_output_exists;
use crate::tools::{resolve_vault_path, resolve_work_path};

use super::controller_parsing::normalize_tool_args;

const TOOL_OUTPUTS_PREFIX: &str = "tool_outputs.";
const GMAIL_DOWNLOAD_ATTACHMENT: &str = "gmail.download_attachment";

pub fn hydrate_tool_args_for_execution(
    tool_name: &str,
    args: Value,
    conversation_id: &str,
    last_step_result: Option<&StepResult>,
    history: &[StepResult],
) -> Value {
    if !tool_name.starts_with(TOOL_OUTPUTS_PREFIX) {
        return args;
    }

    let mut args = normalize_tool_args(args);
    apply_tool_output_arg_defaults(tool_name, &mut args);

    if !tool_outputs_tool_supports_id_hydration(tool_name)
        || value_has_non_empty_string_field(&args, "id")
    {
        return args;
    }

    let output_id = last_step_result
        .and_then(step_result_output_ref_id)
        .or_else(|| history.iter().rev().find_map(step_result_output_ref_id));

    let Some(output_id) = output_id else {
        return args;
    };

    match &mut args {
        Value::Object(map) => {
            map.insert("id".to_string(), Value::String(output_id));
            if tool_outputs_tool_supports_conversation_id(tool_name) {
                let conversation_missing_or_blank = map
                    .get("conversation_id")
                    .map(is_blank_string_value)
                    .unwrap_or(true);
                if conversation_missing_or_blank {
                    map.insert(
                        "conversation_id".to_string(),
                        Value::String(conversation_id.to_string()),
                    );
                }
            }
        }
        _ => {
            args = json!({ "id": output_id });
            if tool_outputs_tool_supports_conversation_id(tool_name) {
                args["conversation_id"] = Value::String(conversation_id.to_string());
            }
        }
    }

    apply_tool_output_arg_defaults(tool_name, &mut args);
    args
}

pub fn hydrate_download_path_for_execution(
    tool_name: &str,
    args: Value,
    db: &crate::db::Db,
) -> Result<Value, String> {
    if tool_name != GMAIL_DOWNLOAD_ATTACHMENT {
        return Ok(args);
    }

    let mut args = normalize_tool_args(args);
    let Some(map) = args.as_object_mut() else {
        return Err("Invalid tool args: expected object".to_string());
    };

    let mut path = map
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let legacy_vault_path = map
        .get("vault_path")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let legacy_work_path = map
        .get("work_path")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if path.is_empty() {
        if !legacy_vault_path.is_empty() {
            let trimmed = legacy_vault_path.trim_start_matches('/');
            path = if trimmed.is_empty() {
                "vault".to_string()
            } else {
                format!("vault/{trimmed}")
            };
        } else if !legacy_work_path.is_empty() {
            path = legacy_work_path;
        }
    }

    let (root, subpath) = if path.is_empty() {
        ("work", ".".to_string())
    } else if path == "vault" {
        ("vault", ".".to_string())
    } else if let Some(rest) = path.strip_prefix("vault/") {
        let trimmed = rest.trim();
        (
            "vault",
            if trimmed.is_empty() {
                ".".to_string()
            } else {
                trimmed.to_string()
            },
        )
    } else {
        ("work", path.clone())
    };

    let resolved = match root {
        "vault" => resolve_vault_path(db, &subpath).map_err(|err| err.message)?,
        "work" => resolve_work_path(db, &subpath).map_err(|err| err.message)?,
        _ => return Err("Invalid root; expected 'vault' or 'work'".to_string()),
    };

    map.insert("root".to_string(), Value::String(root.to_string()));
    map.insert(
        "output_dir".to_string(),
        Value::String(resolved.full_path.to_string_lossy().to_string()),
    );
    map.insert(
        "display_dir".to_string(),
        Value::String(resolved.display_path.clone()),
    );
    if !path.is_empty() {
        map.insert("path".to_string(), Value::String(path));
    }
    map.remove("vault_path");
    map.remove("work_path");

    Ok(args)
}

pub fn validate_tool_execution_preflight(tool_name: &str, args: &Value) -> Result<(), String> {
    validate_tool_outputs_reference_id(tool_name, args)
}

fn validate_tool_outputs_reference_id(tool_name: &str, args: &Value) -> Result<(), String> {
    if !tool_outputs_tool_supports_id_hydration(tool_name) {
        return Ok(());
    }

    let Some(id) = args
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    match tool_output_exists(id) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "Invalid tool_outputs id '{id}': no stored output exists for this id. Use ExecutionId/OutputRef.id from a previous tool execution, or omit id to auto-hydrate from the latest persisted output."
        )),
        Err(err) => Err(format!("Invalid tool_outputs id '{id}': {err}")),
    }
}

fn tool_outputs_tool_supports_id_hydration(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "tool_outputs.read"
            | "tool_outputs.stats"
            | "tool_outputs.extract"
            | "tool_outputs.count"
            | "tool_outputs.sample"
    )
}

fn tool_outputs_tool_supports_conversation_id(tool_name: &str) -> bool {
    matches!(tool_name, "tool_outputs.read")
}

fn apply_tool_output_arg_defaults(tool_name: &str, args: &mut Value) {
    if tool_name == "tool_outputs.extract" {
        ensure_extract_paths_default(args);
    }
}

fn ensure_extract_paths_default(args: &mut Value) {
    if !args.is_object() {
        *args = json!({});
    }

    let Some(map) = args.as_object_mut() else {
        return;
    };

    let default_paths = match map.get("paths") {
        Some(Value::Array(values)) if !values.is_empty() => None,
        Some(Value::String(path)) => {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                Some(vec![Value::String("$".to_string())])
            } else {
                Some(vec![Value::String(trimmed.to_string())])
            }
        }
        _ => Some(vec![Value::String("$".to_string())]),
    };

    if let Some(paths) = default_paths {
        map.insert("paths".to_string(), Value::Array(paths));
    }
}

pub fn step_result_output_ref_id(result: &StepResult) -> Option<String> {
    result
        .output
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
}

pub fn extract_tool_output_ref_id_from_value(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(output_ref_id) = map
                .get("output_ref")
                .and_then(|value| value.get("id"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return Some(output_ref_id.to_string());
            }

            map.values().find_map(extract_tool_output_ref_id_from_value)
        }
        Value::Array(values) => values
            .iter()
            .find_map(extract_tool_output_ref_id_from_value),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|parsed| extract_tool_output_ref_id_from_value(&parsed)),
        _ => None,
    }
}

fn value_has_non_empty_string_field(value: &Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn is_blank_string_value(value: &Value) -> bool {
    value
        .as_str()
        .map(|text| text.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_step_result_with_output_ref(output_ref_id: &str) -> StepResult {
        StepResult {
            step_id: "step-1".to_string(),
            success: true,
            output: Some(json!({
                "output_ref": { "id": output_ref_id }
            })),
            error: None,
            tool_executions: vec![],
            duration_ms: 100,
            completed_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn hydrate_tool_outputs_read_args_uses_last_step_output_ref() {
        let last = make_step_result_with_output_ref("abc-123");
        let args = json!({});
        let result = hydrate_tool_args_for_execution(
            "tool_outputs.read",
            args,
            "conv-1",
            Some(&last),
            &[],
        );
        assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("abc-123"));
    }

    #[test]
    fn hydrate_tool_outputs_read_args_preserves_existing_id() {
        let last = make_step_result_with_output_ref("abc-123");
        let args = json!({ "id": "explicit-id" });
        let result = hydrate_tool_args_for_execution(
            "tool_outputs.read",
            args,
            "conv-1",
            Some(&last),
            &[],
        );
        assert_eq!(
            result.get("id").and_then(|v| v.as_str()),
            Some("explicit-id")
        );
    }

    #[test]
    fn hydrate_tool_outputs_read_args_uses_history_when_last_result_missing() {
        let history = vec![make_step_result_with_output_ref("hist-456")];
        let args = json!({});
        let result = hydrate_tool_args_for_execution(
            "tool_outputs.read",
            args,
            "conv-1",
            None,
            &history,
        );
        assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("hist-456"));
    }

    #[test]
    fn hydrate_tool_outputs_extract_args_uses_last_step_output_ref_and_default_path() {
        let last = make_step_result_with_output_ref("abc-123");
        let args = json!({});
        let result = hydrate_tool_args_for_execution(
            "tool_outputs.extract",
            args,
            "conv-1",
            Some(&last),
            &[],
        );
        assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("abc-123"));
        let paths = result.get("paths").and_then(|v| v.as_array()).expect("paths");
        assert!(!paths.is_empty(), "extract should have default paths");
        assert_eq!(paths[0].as_str(), Some("$"));
    }

    #[test]
    fn hydrate_tool_outputs_list_args_does_not_inject_id() {
        let last = make_step_result_with_output_ref("abc-123");
        let args = json!({});
        let result = hydrate_tool_args_for_execution(
            "tool_outputs.list",
            args,
            "conv-1",
            Some(&last),
            &[],
        );
        assert!(
            result.get("id").is_none(),
            "list tool should not have id injected"
        );
    }

    #[test]
    fn validate_tool_execution_preflight_rejects_unknown_tool_output_id() {
        let args = json!({ "id": "nonexistent-id-12345" });
        let result = validate_tool_execution_preflight("tool_outputs.read", &args);
        assert!(
            result.is_err(),
            "should reject unknown tool output id"
        );
    }

    #[test]
    fn validate_tool_execution_preflight_allows_hydrated_tool_output_id() {
        // When id is missing/empty, preflight should pass (no id to validate)
        let args = json!({});
        let result = validate_tool_execution_preflight("tool_outputs.read", &args);
        assert!(result.is_ok(), "should allow missing id (will be hydrated)");
    }
}
