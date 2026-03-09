use crate::db::Db;
use crate::tools::vault::{ensure_parent_dirs, resolve_root_path};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;

use super::{build_diff_preview, parse_path_content_args, parse_root_arg, read_optional_file, VAULT_PATH_NOTE};

pub(super) fn register_write_tool(
    registry: &mut ToolRegistry,
    db: Db,
    name: &str,
    description: &str,
) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: name.to_string(),
        description: format!("{description}. {VAULT_PATH_NOTE}"),
        args_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "enum": ["vault", "work"] },
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "bytes_written": { "type": "integer" }
            },
            "required": ["path", "bytes_written"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let (path, content) = parse_path_content_args(&args)?;
        let resolved = resolve_root_path(&handler_db, root, &path)?;
        ensure_parent_dirs(&resolved.full_path)?;
        fs::write(&resolved.full_path, content.as_bytes())
            .map_err(|err| ToolError::new(format!("Failed to write file: {err}")))?;
        Ok(json!({
            "path": resolved.display_path,
            "bytes_written": content.len()
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let (path, content) = parse_path_content_args(&args)?;
        let resolved = resolve_root_path(&preview_db, root, &path)?;
        let before = read_optional_file(&resolved.full_path)?;
        Ok(build_diff_preview(
            &resolved.display_path,
            &before,
            &content,
        ))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}
