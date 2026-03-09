use crate::db::Db;
use crate::tools::vault::resolve_root_path;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use reqwest::blocking::multipart;
use serde_json::{json, Value};
use std::sync::Arc;

use super::{
    anthropic_client, extract_anthropic_error, get_anthropic_api_key, require_string,
    ANTHROPIC_BASE_URL, ANTHROPIC_BETA_FILES, ANTHROPIC_VERSION, DEFAULT_TIMEOUT_SECS,
};

pub(super) fn register_file_upload(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.file_upload".to_string(),
        description: "Upload a local file to the Anthropic Files API. Returns a file_id that can be referenced in batch requests.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to root)"
                },
                "root": {
                    "type": "string",
                    "enum": ["vault", "work"],
                    "description": "Root directory (default: work)"
                },
                "mime_type": {
                    "type": "string",
                    "description": "MIME type override (default: auto-detect from extension)"
                }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "file_id": { "type": "string" },
                "filename": { "type": "string" },
                "size_bytes": { "type": "integer" },
                "created_at": { "type": "string" }
            },
            "required": ["file_id", "filename", "size_bytes", "created_at"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let path_input = require_string(&args, "path")?;
        let root = args
            .get("root")
            .and_then(|v| v.as_str())
            .unwrap_or("work");

        let resolved = resolve_root_path(&db, root, &path_input)?;
        if !resolved.full_path.is_file() {
            return Err(ToolError::new(format!(
                "File not found: {}",
                resolved.display_path
            )));
        }

        let filename = resolved
            .full_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let mime_type = args
            .get("mime_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                mime_guess::from_path(&resolved.full_path)
                    .first_or_octet_stream()
                    .to_string()
            });

        let file_bytes = std::fs::read(&resolved.full_path)
            .map_err(|e| ToolError::new(format!("Failed to read file: {e}")))?;

        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let part = multipart::Part::bytes(file_bytes)
            .file_name(filename.clone())
            .mime_str(&mime_type)
            .map_err(|e| ToolError::new(format!("Invalid MIME type: {e}")))?;
        let form = multipart::Form::new().part("file", part);

        let response = client
            .post(format!("{ANTHROPIC_BASE_URL}/v1/files"))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", ANTHROPIC_BETA_FILES)
            .multipart(form)
            .send()
            .map_err(|e| ToolError::new(format!("Upload request failed: {e}")))?;

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
            "file_id": body.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::new("Anthropic API response missing 'id' field"))?,
            "filename": body.get("filename").and_then(|v| v.as_str()).unwrap_or(&filename),
            "size_bytes": body.get("size_bytes").and_then(|v| v.as_i64()).unwrap_or(0),
            "created_at": body.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
