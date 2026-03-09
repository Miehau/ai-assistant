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

pub(super) fn register_file_delete(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.file_delete".to_string(),
        description: "Delete a file from the Anthropic Files API.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "file_id": { "type": "string" }
            },
            "required": ["file_id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "file_id": { "type": "string" },
                "deleted": { "type": "boolean" }
            },
            "required": ["file_id", "deleted"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let file_id = validate_id(&require_string(&args, "file_id")?, "file_id")?;
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let response = client
            .delete(format!("{ANTHROPIC_BASE_URL}/v1/files/{file_id}"))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", ANTHROPIC_BETA_FILES)
            .send()
            .map_err(|e| ToolError::new(format!("File delete request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Files API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body: Value = response
            .json()
            .map_err(|e| ToolError::new(format!("Failed to parse response: {e}")))?;

        Ok(json!({
            "file_id": body.get("id").and_then(|v| v.as_str()).unwrap_or(&file_id),
            "deleted": body.get("deleted").and_then(|v| v.as_bool()).unwrap_or(true),
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
