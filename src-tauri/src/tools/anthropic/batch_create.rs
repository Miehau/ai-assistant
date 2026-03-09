use crate::db::Db;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

use super::{
    anthropic_client, extract_anthropic_error, get_anthropic_api_key, ANTHROPIC_BASE_URL,
    ANTHROPIC_VERSION, DEFAULT_TIMEOUT_SECS,
};

pub(super) fn register_batch_create(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_create".to_string(),
        description: "Create a Message Batch on the Anthropic API. Each request in the batch is a standard Messages API call. Returns a batch_id for tracking.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "requests": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "custom_id": { "type": "string" },
                            "params": {
                                "type": "object",
                                "properties": {
                                    "model": { "type": "string" },
                                    "max_tokens": { "type": "integer" },
                                    "system": { "type": "string" },
                                    "messages": { "type": "array" },
                                    "temperature": { "type": "number" }
                                },
                                "required": ["model", "max_tokens", "messages"]
                            }
                        },
                        "required": ["custom_id", "params"]
                    }
                }
            },
            "required": ["requests"],
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
                "expires_at": { "type": "string" }
            },
            "required": ["batch_id", "status"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let requests = args
            .get("requests")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::new("Missing required field: requests"))?;

        if requests.is_empty() {
            return Err(ToolError::new("requests array must not be empty"));
        }

        for (i, req) in requests.iter().enumerate() {
            let custom_id = req
                .get("custom_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if custom_id.trim().is_empty() {
                return Err(ToolError::new(format!(
                    "requests[{i}].custom_id must not be empty"
                )));
            }
        }

        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let body = json!({ "requests": requests });

        let response = client
            .post(format!("{ANTHROPIC_BASE_URL}/v1/messages/batches"))
            .header("Content-Type", "application/json")
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .map_err(|e| ToolError::new(format!("Batch create request failed: {e}")))?;

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

        Ok(json!({
            "batch_id": body.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::new("Anthropic API response missing 'id' field"))?,
            "status": body.get("processing_status").and_then(|v| v.as_str()).unwrap_or("unknown"),
            "request_counts": body.get("request_counts").cloned().unwrap_or(json!({})),
            "created_at": body.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            "expires_at": body.get("expires_at").and_then(|v| v.as_str()).unwrap_or(""),
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
