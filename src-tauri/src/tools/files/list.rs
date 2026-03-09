use crate::db::Db;
use crate::tools::vault::{get_vault_root, get_work_root, resolve_root_path};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

use super::{list_dir, optional_string_arg, parse_root_arg, VAULT_PATH_NOTE};

pub(super) fn register_list_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.list".to_string(),
        description: format!("List files and folders under a vault path. {VAULT_PATH_NOTE}"),
        args_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "enum": ["vault", "work"] },
                "path": { "type": "string" },
                "depth": { "type": "integer", "minimum": 0 },
                "include_files": { "type": "boolean" },
                "include_dirs": { "type": "boolean" }
            },
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "entries": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "type": { "type": "string", "enum": ["file", "dir"] }
                        },
                        "required": ["path", "type"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["path", "entries"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
        let include_files = args
            .get("include_files")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let include_dirs = args
            .get("include_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let root = parse_root_arg(&args)?;
        let requested_path = optional_string_arg(&args, "path").unwrap_or_default();
        let base_root = match root {
            "vault" => get_vault_root(&db)?,
            "work" => get_work_root(&db)?,
            _ => return Err(ToolError::new("Invalid root; expected 'vault' or 'work'")),
        };
        let (root_path, display_path) = if requested_path.trim().is_empty() {
            let display = crate::tools::vault::to_display_path(&base_root, &base_root);
            let display = if display.is_empty() {
                ".".to_string()
            } else {
                display
            };
            (base_root.clone(), display)
        } else {
            let resolved = resolve_root_path(&db, root, &requested_path)?;
            (resolved.full_path, resolved.display_path)
        };

        if !root_path.is_dir() {
            return Err(ToolError::new("Path is not a directory"));
        }

        let mut entries: Vec<Value> = Vec::new();
        list_dir(
            &base_root,
            &root_path,
            depth,
            include_files,
            include_dirs,
            &mut entries,
        )?;

        Ok(json!({
            "path": display_path,
            "entries": entries
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
