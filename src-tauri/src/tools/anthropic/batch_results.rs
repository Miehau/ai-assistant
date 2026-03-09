use crate::db::Db;
use crate::tools::vault::resolve_root_path;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

use super::{
    anthropic_client, extract_anthropic_error, get_anthropic_api_key, require_string, validate_id,
    ANTHROPIC_BASE_URL, ANTHROPIC_VERSION, RESULTS_TIMEOUT_SECS,
};

pub(super) fn register_batch_results(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_results".to_string(),
        description: "Fetch results of a completed Anthropic Message Batch. Returns JSONL data saved to the specified path or persisted via the tool outputs system.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "save_to": {
                    "type": "string",
                    "description": "Vault-relative path to save results as JSONL. If omitted, results are persisted via the tool outputs system."
                }
            },
            "required": ["batch_id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "result_count": { "type": "integer" },
                "saved_to": { "type": "string" },
                "results": { "type": "array" }
            },
            "required": ["batch_id", "result_count"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Persist,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let batch_id = validate_id(&require_string(&args, "batch_id")?, "batch_id")?;
        let save_to = args.get("save_to").and_then(|v| v.as_str());
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(RESULTS_TIMEOUT_SECS)?;

        let response = client
            .get(format!(
                "{ANTHROPIC_BASE_URL}/v1/messages/batches/{batch_id}/results"
            ))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .map_err(|e| ToolError::new(format!("Batch results request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Batch API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body = response
            .text()
            .map_err(|e| ToolError::new(format!("Failed to read results body: {e}")))?;

        let mut results: Vec<Value> = Vec::new();
        let mut parse_errors = 0usize;
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(value) => results.push(value),
                Err(_) => parse_errors += 1,
            }
        }
        let result_count = results.len();

        if let Some(save_path) = save_to {
            let resolved = resolve_root_path(&db, "vault", save_path)?;
            if let Some(parent) = resolved.full_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ToolError::new(format!("Failed to create directories: {e}")))?;
            }
            std::fs::write(&resolved.full_path, &body)
                .map_err(|e| ToolError::new(format!("Failed to write results: {e}")))?;

            let mut result = json!({
                "batch_id": batch_id,
                "result_count": result_count,
                "saved_to": resolved.display_path,
            });
            if parse_errors > 0 {
                result["parse_errors"] = json!(parse_errors);
            }
            Ok(result)
        } else {
            let mut result = json!({
                "batch_id": batch_id,
                "result_count": result_count,
                "results": results,
            });
            if parse_errors > 0 {
                result["parse_errors"] = json!(parse_errors);
            }
            Ok(result)
        }
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
