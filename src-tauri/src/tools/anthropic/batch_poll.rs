use crate::db::Db;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

use super::{
    anthropic_client, extract_anthropic_error, get_anthropic_api_key, require_string, validate_id,
    ANTHROPIC_BASE_URL, ANTHROPIC_VERSION, DEFAULT_TIMEOUT_SECS,
};

pub(super) fn register_batch_poll(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_poll".to_string(),
        description: "Check the status of an Anthropic Message Batch.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" }
            },
            "required": ["batch_id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "status": { "type": "string" },
                "request_counts": {
                    "type": "object",
                    "properties": {
                        "processing": { "type": "integer" },
                        "succeeded": { "type": "integer" },
                        "errored": { "type": "integer" },
                        "canceled": { "type": "integer" },
                        "expired": { "type": "integer" }
                    }
                },
                "created_at": { "type": "string" },
                "expires_at": { "type": "string" },
                "ended_at": { "type": "string" }
            },
            "required": ["batch_id", "status"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let batch_id = validate_id(&require_string(&args, "batch_id")?, "batch_id")?;
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let response = client
            .get(format!(
                "{ANTHROPIC_BASE_URL}/v1/messages/batches/{batch_id}"
            ))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .map_err(|e| ToolError::new(format!("Batch poll request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Batch API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body: Value = response
            .json()
            .map_err(|e| ToolError::new(format!("Failed to parse response: {e}")))?;

        let mut result = json!({
            "batch_id": body.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "status": body.get("processing_status").and_then(|v| v.as_str()).unwrap_or("unknown"),
            "request_counts": body.get("request_counts").cloned().unwrap_or(json!({})),
            "created_at": body.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            "expires_at": body.get("expires_at").and_then(|v| v.as_str()).unwrap_or(""),
        });

        if let Some(ended_at) = body.get("ended_at").and_then(|v| v.as_str()) {
            result["ended_at"] = json!(ended_at);
        }

        Ok(result)
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
