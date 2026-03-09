use crate::db::Db;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

use super::{
    anthropic_client, extract_anthropic_error, get_anthropic_api_key, require_string, validate_id,
    ANTHROPIC_BASE_URL, ANTHROPIC_BETA_FILES, ANTHROPIC_VERSION, DEFAULT_TIMEOUT_SECS,
};

pub(super) fn register_batch_process_files(
    registry: &mut ToolRegistry,
    db: Db,
) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_process_files".to_string(),
        description: "High-level batch tool: takes a list of uploaded file_ids and a shared prompt, constructs one Messages API request per file, and submits them as a batch. Returns a batch_id for tracking. Use this instead of anthropic.batch_create when processing multiple files with the same prompt.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "file_ids": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "file_id": { "type": "string" },
                            "label": {
                                "type": "string",
                                "description": "Human-readable label used as custom_id (e.g. original filename)"
                            }
                        },
                        "required": ["file_id", "label"]
                    },
                    "minItems": 1,
                    "description": "List of Anthropic file_ids with labels (from anthropic.file_upload)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to apply to each file"
                },
                "system": {
                    "type": "string",
                    "description": "Optional system prompt for each request"
                },
                "model": {
                    "type": "string",
                    "description": "Model to use (default: claude-sonnet-4-6)"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Max tokens per response (default: 4096)"
                }
            },
            "required": ["file_ids", "prompt"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "status": { "type": "string" },
                "request_count": { "type": "integer" },
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
            "required": ["batch_id", "status", "request_count"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let file_entries = args
            .get("file_ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::new("Missing required field: file_ids"))?;

        if file_entries.is_empty() {
            return Err(ToolError::new("file_ids array must not be empty"));
        }

        let prompt = require_string(&args, "prompt")?;
        let system = args.get("system").and_then(|v| v.as_str());
        let model = args
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-6");
        let max_tokens = args
            .get("max_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(4096);

        let mut requests = Vec::with_capacity(file_entries.len());
        for (i, entry) in file_entries.iter().enumerate() {
            let file_id = entry
                .get("file_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::new(format!("file_ids[{i}].file_id is required"))
                })?;
            let label = entry
                .get("label")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::new(format!("file_ids[{i}].label is required"))
                })?;

            validate_id(file_id, &format!("file_ids[{i}].file_id"))?;

            let mut content = vec![
                json!({
                    "type": "document",
                    "source": {
                        "type": "file",
                        "file_id": file_id
                    }
                }),
                json!({
                    "type": "text",
                    "text": prompt
                }),
            ];
            let _ = &mut content; // satisfy borrow checker

            let mut params = json!({
                "model": model,
                "max_tokens": max_tokens,
                "messages": [{
                    "role": "user",
                    "content": content
                }]
            });

            if let Some(system_prompt) = system {
                params["system"] = json!(system_prompt);
            }

            let custom_id: String = label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                .take(64)
                .collect();

            requests.push(json!({
                "custom_id": custom_id,
                "params": params
            }));
        }

        let request_count = requests.len();
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let body = json!({ "requests": requests });

        let response = client
            .post(format!("{ANTHROPIC_BASE_URL}/v1/messages/batches"))
            .header("Content-Type", "application/json")
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", ANTHROPIC_BETA_FILES)
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
            "request_count": request_count,
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
