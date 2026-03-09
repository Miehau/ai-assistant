use crate::db::Db;
use crate::tools::vault::resolve_root_path;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;

use super::{apply_line_edit, build_diff_preview, parse_edit_args, parse_root_arg, VAULT_PATH_NOTE};

pub(super) fn register_edit_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.edit".to_string(),
        description: format!("Edit a file by replacing a line range. {VAULT_PATH_NOTE}"),
        args_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "enum": ["vault", "work"] },
                "path": { "type": "string" },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "content": { "type": "string" }
            },
            "required": ["path", "start_line", "end_line", "content"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "updated": { "type": "boolean" }
            },
            "required": ["path", "updated"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let edit = parse_edit_args(&args)?;
        let resolved = resolve_root_path(&handler_db, root, &edit.path)?;
        let original = fs::read_to_string(&resolved.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let updated = apply_line_edit(&original, edit.start_line, edit.end_line, &edit.content)?;
        fs::write(&resolved.full_path, updated.as_bytes())
            .map_err(|err| ToolError::new(format!("Failed to edit file: {err}")))?;
        Ok(json!({
            "path": resolved.display_path,
            "updated": true
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let edit = parse_edit_args(&args)?;
        let resolved = resolve_root_path(&preview_db, root, &edit.path)?;
        let original = fs::read_to_string(&resolved.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let updated = apply_line_edit(&original, edit.start_line, edit.end_line, &edit.content)?;
        Ok(build_diff_preview(
            &resolved.display_path,
            &original,
            &updated,
        ))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}
