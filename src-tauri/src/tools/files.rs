use crate::db::Db;
use crate::tools::vault::{ensure_parent_dirs, resolve_vault_path};
use crate::tools::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry};
use serde_json::{json, Value};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

pub fn register_file_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    register_read_tool(registry, db.clone(), "files.read", "Read file contents")?;
    register_read_tool(registry, db.clone(), "files.open", "Open file contents")?;
    register_write_tool(registry, db.clone(), "files.write", "Write/replace file contents")?;
    register_write_tool(registry, db.clone(), "files.replace", "Replace file contents")?;
    register_append_tool(registry, db.clone())?;
    register_create_tool(registry, db.clone())?;
    register_edit_tool(registry, db)?;
    Ok(())
}

fn register_read_tool(
    registry: &mut ToolRegistry,
    db: Db,
    name: &str,
    description: &str,
) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: name.to_string(),
        description: description.to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        }),
        requires_approval: false,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let path = require_string_arg(&args, "path")?;
        let vault_path = resolve_vault_path(&db, &path)?;
        let content = fs::read_to_string(&vault_path.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        Ok(json!({
            "path": vault_path.display_path,
            "content": content
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_write_tool(registry: &mut ToolRegistry, db: Db, name: &str, description: &str) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: name.to_string(),
        description: description.to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
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
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let (path, content) = parse_path_content_args(&args)?;
        let vault_path = resolve_vault_path(&handler_db, &path)?;
        ensure_parent_dirs(&vault_path.full_path)?;
        fs::write(&vault_path.full_path, content.as_bytes())
            .map_err(|err| ToolError::new(format!("Failed to write file: {err}")))?;
        Ok(json!({
            "path": vault_path.display_path,
            "bytes_written": content.len()
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let (path, content) = parse_path_content_args(&args)?;
        let vault_path = resolve_vault_path(&preview_db, &path)?;
        let before = read_optional_file(&vault_path.full_path)?;
        Ok(build_diff_preview(&vault_path.display_path, &before, &content))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}

fn register_append_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.append".to_string(),
        description: "Append content to a file".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
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
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let (path, content) = parse_path_content_args(&args)?;
        let vault_path = resolve_vault_path(&db, &path)?;
        ensure_parent_dirs(&vault_path.full_path)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&vault_path.full_path)
            .map_err(|err| ToolError::new(format!("Failed to open file: {err}")))?;
        file.write_all(content.as_bytes())
            .map_err(|err| ToolError::new(format!("Failed to append file: {err}")))?;
        Ok(json!({
            "path": vault_path.display_path,
            "bytes_written": content.len()
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_create_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.create".to_string(),
        description: "Create a new file".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
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
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let path = require_string_arg(&args, "path")?;
        let content = optional_string_arg(&args, "content");
        let vault_path = resolve_vault_path(&handler_db, &path)?;
        if vault_path.full_path.exists() {
            return Err(ToolError::new("File already exists"));
        }
        ensure_parent_dirs(&vault_path.full_path)?;
        match content {
            Some(content) => {
                fs::write(&vault_path.full_path, content.as_bytes())
                    .map_err(|err| ToolError::new(format!("Failed to create file: {err}")))?;
            }
            None => {
                fs::File::create(&vault_path.full_path)
                    .map_err(|err| ToolError::new(format!("Failed to create file: {err}")))?;
            }
        }
        Ok(json!({
            "path": vault_path.display_path,
            "created": true
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let path = require_string_arg(&args, "path")?;
        let content = optional_string_arg(&args, "content").unwrap_or_default();
        let vault_path = resolve_vault_path(&preview_db, &path)?;
        if vault_path.full_path.exists() {
            return Err(ToolError::new("File already exists"));
        }
        Ok(build_diff_preview(&vault_path.display_path, "", &content))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}

fn register_edit_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.edit".to_string(),
        description: "Edit a file by replacing a line range".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
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
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let edit = parse_edit_args(&args)?;
        let vault_path = resolve_vault_path(&handler_db, &edit.path)?;
        let original = fs::read_to_string(&vault_path.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let updated = apply_line_edit(&original, edit.start_line, edit.end_line, &edit.content)?;
        fs::write(&vault_path.full_path, updated.as_bytes())
            .map_err(|err| ToolError::new(format!("Failed to edit file: {err}")))?;
        Ok(json!({
            "path": vault_path.display_path,
            "updated": true
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let edit = parse_edit_args(&args)?;
        let vault_path = resolve_vault_path(&preview_db, &edit.path)?;
        let original = fs::read_to_string(&vault_path.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let updated = apply_line_edit(&original, edit.start_line, edit.end_line, &edit.content)?;
        Ok(build_diff_preview(&vault_path.display_path, &original, &updated))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}

struct EditArgs {
    path: String,
    start_line: usize,
    end_line: usize,
    content: String,
}

fn require_string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing or invalid '{key}'")))
}

fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn parse_path_content_args(args: &Value) -> Result<(String, String), ToolError> {
    let path = require_string_arg(args, "path")?;
    let content = require_string_arg(args, "content")?;
    Ok((path, content))
}

fn parse_edit_args(args: &Value) -> Result<EditArgs, ToolError> {
    let path = require_string_arg(args, "path")?;
    let start_line = args
        .get("start_line")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| ToolError::new("Missing or invalid 'start_line'"))? as usize;
    let end_line = args
        .get("end_line")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| ToolError::new("Missing or invalid 'end_line'"))? as usize;
    let content = require_string_arg(args, "content")?;
    Ok(EditArgs {
        path,
        start_line,
        end_line,
        content,
    })
}

fn apply_line_edit(
    original: &str,
    start_line: usize,
    end_line: usize,
    replacement: &str,
) -> Result<String, ToolError> {
    if start_line == 0 || end_line == 0 || end_line < start_line {
        return Err(ToolError::new("Invalid line range"));
    }

    let has_trailing_newline = original.ends_with('\n');
    let mut lines: Vec<String> = original.lines().map(|line| line.to_string()).collect();

    if lines.is_empty() {
        return Err(ToolError::new("File is empty"));
    }

    if end_line > lines.len() {
        return Err(ToolError::new("Line range exceeds file length"));
    }

    let replacement_lines = replacement
        .split('\n')
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    lines.splice(start_line - 1..end_line, replacement_lines);

    let mut updated = lines.join("\n");
    if has_trailing_newline {
        updated.push('\n');
    }
    Ok(updated)
}

fn read_optional_file(path: &Path) -> Result<String, ToolError> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path)
        .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))
}

fn build_diff_preview(path: &str, before: &str, after: &str) -> Value {
    let diff = format!(
        "--- a/{path}\n+++ b/{path}\n@@\n-{before}\n+{after}"
    );
    json!({
        "path": path,
        "before": before,
        "after": after,
        "diff": diff
    })
}
