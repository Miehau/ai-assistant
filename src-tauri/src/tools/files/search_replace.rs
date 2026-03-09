use crate::db::Db;
use crate::tools::vault::resolve_root_path;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;

use super::{
    apply_search_replace, build_diff_preview, parse_root_arg, parse_search_replace_args,
    VAULT_PATH_NOTE,
};

pub(super) fn register_search_replace_tool(
    registry: &mut ToolRegistry,
    db: Db,
) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.search_replace".to_string(),
        description: format!(
            "Search and replace within a file (defaults: literal=true, case_sensitive=true). {VAULT_PATH_NOTE}"
        ),
        args_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "enum": ["vault", "work"] },
                "path": { "type": "string" },
                "query": { "type": "string" },
                "replace": { "type": "string" },
                "literal": { "type": "boolean" },
                "case_sensitive": { "type": "boolean" },
                "max_replacements": { "type": "integer", "minimum": 1 }
            },
            "required": ["path", "query", "replace"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "updated": { "type": "boolean" },
                "replacements": { "type": "integer" }
            },
            "required": ["path", "updated", "replacements"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let edit = parse_search_replace_args(&args)?;
        let resolved = resolve_root_path(&handler_db, root, &edit.path)?;
        let original = fs::read_to_string(&resolved.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let (updated, replacements) = apply_search_replace(&original, &edit)?;
        if replacements > 0 {
            fs::write(&resolved.full_path, updated.as_bytes())
                .map_err(|err| ToolError::new(format!("Failed to write file: {err}")))?;
        }
        Ok(json!({
            "path": resolved.display_path,
            "updated": replacements > 0,
            "replacements": replacements as i64
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let edit = parse_search_replace_args(&args)?;
        let resolved = resolve_root_path(&preview_db, root, &edit.path)?;
        let original = fs::read_to_string(&resolved.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let (updated, replacements) = apply_search_replace(&original, &edit)?;
        let mut preview = build_diff_preview(&resolved.display_path, &original, &updated);
        if let Some(obj) = preview.as_object_mut() {
            obj.insert("replacements".to_string(), json!(replacements as i64));
            if replacements == 0 {
                obj.insert("note".to_string(), json!("No matches found"));
            }
        }
        Ok(preview)
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}
