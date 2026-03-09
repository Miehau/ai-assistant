use crate::db::Db;
use crate::tools::vault::resolve_root_path;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::sync::Arc;

use super::{parse_root_arg, require_string_arg, DEFAULT_READ_MAX_CHARS, DEFAULT_READ_MAX_LINES, VAULT_PATH_NOTE};

pub(super) fn register_read_range_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.read_range".to_string(),
        description: format!(
            "Read a range of lines from a file (defaults: start_line=1, max_lines={DEFAULT_READ_MAX_LINES}, max_chars={DEFAULT_READ_MAX_CHARS}). {VAULT_PATH_NOTE}"
        ),
        args_schema: json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "enum": ["vault", "work"] },
                "path": { "type": "string" },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "max_lines": { "type": "integer", "minimum": 1 },
                "max_chars": { "type": "integer", "minimum": 1 }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "start_line": { "type": "integer" },
                "end_line": { "type": "integer" },
                "content": { "type": "string" },
                "truncated": { "type": "boolean" }
            },
            "required": ["path", "start_line", "end_line", "content", "truncated"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = parse_root_arg(&args)?;
        let path = require_string_arg(&args, "path")?;
        let start_line = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        if start_line == 0 {
            return Err(ToolError::new("Invalid 'start_line'"));
        }
        let end_line = args
            .get("end_line")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let max_lines = args
            .get("max_lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_READ_MAX_LINES as u64) as usize;
        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_READ_MAX_CHARS as u64) as usize;

        if let Some(end_line) = end_line {
            if end_line < start_line {
                return Err(ToolError::new("Invalid line range"));
            }
        }
        if max_lines == 0 {
            return Err(ToolError::new("Invalid 'max_lines'"));
        }
        if max_chars == 0 {
            return Err(ToolError::new("Invalid 'max_chars'"));
        }

        let requested_end_line =
            end_line.unwrap_or_else(|| start_line.saturating_add(max_lines.saturating_sub(1)));

        let resolved = resolve_root_path(&db, root, &path)?;
        let file = std::fs::File::open(&resolved.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let mut reader = BufReader::new(file);

        let mut line_no = 0usize;
        let mut content = String::new();
        let mut truncated = false;
        let mut last_line_included: Option<usize> = None;
        let mut buf = String::new();

        loop {
            buf.clear();
            let bytes = reader
                .read_line(&mut buf)
                .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
            if bytes == 0 {
                break;
            }
            line_no += 1;
            if line_no < start_line {
                continue;
            }
            if line_no > requested_end_line {
                break;
            }

            if content.len().saturating_add(buf.len()) > max_chars {
                let remaining = max_chars.saturating_sub(content.len());
                if remaining > 0 {
                    content.push_str(&buf.chars().take(remaining).collect::<String>());
                }
                truncated = true;
                last_line_included = Some(line_no);
                break;
            }

            content.push_str(&buf);
            last_line_included = Some(line_no);
        }

        if start_line > line_no && last_line_included.is_none() {
            if line_no == 0 {
                return Err(ToolError::new("File is empty"));
            }
            return Err(ToolError::new("start_line exceeds file length"));
        }

        let end_line_actual = last_line_included.unwrap_or(start_line);

        Ok(json!({
            "path": resolved.display_path,
            "start_line": start_line as i64,
            "end_line": end_line_actual as i64,
            "content": content,
            "truncated": truncated
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
