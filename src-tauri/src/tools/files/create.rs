use crate::db::Db;
use crate::tools::vault::{ensure_parent_dirs, resolve_root_path};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;

use super::{build_diff_preview, optional_string_arg, parse_root_arg, require_string_arg, VAULT_PATH_NOTE};

pub(super) fn register_create_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.create".to_string(),
        description: format!("Create a new file. {VAULT_PATH_NOTE}"),
        args_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "enum": ["vault", "work"] },
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "created": { "type": "boolean" }
            },
            "required": ["path", "created"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let path = require_string_arg(&args, "path")?;
        let content = optional_string_arg(&args, "content");
        let resolved = resolve_root_path(&handler_db, root, &path)?;
        if resolved.full_path.exists() {
            return Err(ToolError::new("File already exists"));
        }
        ensure_parent_dirs(&resolved.full_path)?;
        match content {
            Some(content) => {
                fs::write(&resolved.full_path, content.as_bytes())
                    .map_err(|err| ToolError::new(format!("Failed to create file: {err}")))?;
            }
            None => {
                fs::File::create(&resolved.full_path)
                    .map_err(|err| ToolError::new(format!("Failed to create file: {err}")))?;
            }
        }
        Ok(json!({
            "path": resolved.display_path,
            "created": true
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let path = require_string_arg(&args, "path")?;
        let content = optional_string_arg(&args, "content").unwrap_or_default();
        let resolved = resolve_root_path(&preview_db, root, &path)?;
        if resolved.full_path.exists() {
            return Err(ToolError::new("File already exists"));
        }
        Ok(build_diff_preview(&resolved.display_path, "", &content))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}
