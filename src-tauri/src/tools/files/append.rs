use crate::db::Db;
use crate::tools::vault::{ensure_parent_dirs, resolve_root_path};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;

use super::{parse_path_content_args, parse_root_arg, VAULT_PATH_NOTE};

pub(super) fn register_append_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.append".to_string(),
        description: format!("Append content to a file. {VAULT_PATH_NOTE}"),
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
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let (path, content) = parse_path_content_args(&args)?;
        let resolved = resolve_root_path(&db, root, &path)?;
        ensure_parent_dirs(&resolved.full_path)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved.full_path)
            .map_err(|err| ToolError::new(format!("Failed to open file: {err}")))?;
        file.write_all(content.as_bytes())
            .map_err(|err| ToolError::new(format!("Failed to append file: {err}")))?;
        Ok(json!({
            "path": resolved.display_path,
            "bytes_written": content.len()
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
