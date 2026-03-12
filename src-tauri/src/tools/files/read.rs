use crate::db::Db;
use crate::tools::vault::resolve_root_path;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::{detect_media_type, parse_root_arg, require_string_arg, MAX_READ_FILE_SIZE, MAX_READ_IMAGE_SIZE, VAULT_PATH_NOTE};

pub(super) fn register_read_tool(
    registry: &mut ToolRegistry,
    db: Db,
    name: &str,
    description: &str,
) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: name.to_string(),
        description: format!("{description}. Files larger than 64 KB must be read with files.read_range. {VAULT_PATH_NOTE}"),
        args_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "enum": ["vault", "work"] },
                "path": { "type": "string" },
                "as_type": { "type": "string", "enum": ["text", "base64"], "default": "text" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "media_type": { "type": "string" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let path = require_string_arg(&args, "path")?;
        let as_type = args
            .get("as_type")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        let resolved = resolve_root_path(&db, root, &path)?;
        let metadata = std::fs::metadata(&resolved.full_path)
            .map_err(|err| ToolError::new(format!("Failed to stat file: {err}")))?;

        match as_type {
            "text" => {
                if metadata.len() > MAX_READ_FILE_SIZE {
                    return Err(ToolError::new(format!(
                        "File is too large ({} bytes, limit is {} bytes). Use files.read_range to read it in sections.",
                        metadata.len(), MAX_READ_FILE_SIZE
                    )));
                }
                let content = fs::read_to_string(&resolved.full_path)
                    .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
                Ok(json!({
                    "path": resolved.display_path,
                    "content": content
                }))
            }
            "base64" => {
                if metadata.len() > MAX_READ_IMAGE_SIZE {
                    return Err(ToolError::new(format!(
                        "File is too large ({} bytes, limit is {} bytes).",
                        metadata.len(), MAX_READ_IMAGE_SIZE
                    )));
                }
                let bytes = fs::read(&resolved.full_path)
                    .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
                let content = general_purpose::STANDARD.encode(&bytes);
                let media_type = detect_media_type(Path::new(&resolved.full_path))
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                Ok(json!({
                    "path": resolved.display_path,
                    "content": content,
                    "media_type": media_type
                }))
            }
            _ => Err(ToolError::new("Invalid as_type; expected 'text' or 'base64'")),
        }
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
