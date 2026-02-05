use crate::db::Db;
use crate::tools::vault::{ensure_parent_dirs, resolve_vault_path};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use regex::{Regex, RegexBuilder};
use serde_json::{json, Value};
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Arc;

const VAULT_PATH_NOTE: &str =
    "Paths are relative to the vault root (use \".\" for root; no absolute paths).";
const DEFAULT_READ_MAX_LINES: usize = 200;
const DEFAULT_READ_MAX_CHARS: usize = 20_000;

pub fn register_file_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    register_list_tool(registry, db.clone())?;
    register_read_tool(registry, db.clone(), "files.read", "Read file contents")?;
    register_read_tool(registry, db.clone(), "files.open", "Open file contents")?;
    register_read_range_tool(registry, db.clone())?;
    register_search_replace_tool(registry, db.clone())?;
    register_write_tool(
        registry,
        db.clone(),
        "files.write",
        "Write/replace file contents",
    )?;
    register_write_tool(
        registry,
        db.clone(),
        "files.replace",
        "Replace file contents",
    )?;
    register_append_tool(registry, db.clone())?;
    register_create_tool(registry, db.clone())?;
    register_edit_tool(registry, db)?;
    Ok(())
}

fn register_list_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.list".to_string(),
        description: format!("List files and folders under a vault path. {VAULT_PATH_NOTE}"),
        args_schema: json!({
            "type": "object",
            "properties": {
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
        let requested_path = optional_string_arg(&args, "path").unwrap_or_default();
        let vault_root = crate::tools::vault::get_vault_root(&db)?;
        let (root_path, display_path) = if requested_path.trim().is_empty() {
            let display = crate::tools::vault::to_display_path(&vault_root, &vault_root);
            let display = if display.is_empty() {
                ".".to_string()
            } else {
                display
            };
            (vault_root.clone(), display)
        } else {
            let vault_path = resolve_vault_path(&db, &requested_path)?;
            (vault_path.full_path, vault_path.display_path)
        };

        if !root_path.is_dir() {
            return Err(ToolError::new("Path is not a directory"));
        }

        let mut entries: Vec<Value> = Vec::new();
        list_dir(
            &vault_root,
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

fn register_read_tool(
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
        result_mode: ToolResultMode::Auto,
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

fn register_read_range_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.read_range".to_string(),
        description: format!(
            "Read a range of lines from a file (defaults: start_line=1, max_lines={DEFAULT_READ_MAX_LINES}, max_chars={DEFAULT_READ_MAX_CHARS}). {VAULT_PATH_NOTE}"
        ),
        args_schema: json!({
            "type": "object",
            "properties": {
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

        let vault_path = resolve_vault_path(&db, &path)?;
        let file = fs::File::open(&vault_path.full_path)
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
            "path": vault_path.display_path,
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

fn register_search_replace_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.search_replace".to_string(),
        description: format!(
            "Search and replace within a file (defaults: literal=true, case_sensitive=true). {VAULT_PATH_NOTE}"
        ),
        args_schema: json!({
            "type": "object",
            "properties": {
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
        let edit = parse_search_replace_args(&args)?;
        let vault_path = resolve_vault_path(&handler_db, &edit.path)?;
        let original = fs::read_to_string(&vault_path.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let (updated, replacements) = apply_search_replace(&original, &edit)?;
        if replacements > 0 {
            fs::write(&vault_path.full_path, updated.as_bytes())
                .map_err(|err| ToolError::new(format!("Failed to write file: {err}")))?;
        }
        Ok(json!({
            "path": vault_path.display_path,
            "updated": replacements > 0,
            "replacements": replacements as i64
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let edit = parse_search_replace_args(&args)?;
        let vault_path = resolve_vault_path(&preview_db, &edit.path)?;
        let original = fs::read_to_string(&vault_path.full_path)
            .map_err(|err| ToolError::new(format!("Failed to read file: {err}")))?;
        let (updated, replacements) = apply_search_replace(&original, &edit)?;
        let mut preview = build_diff_preview(&vault_path.display_path, &original, &updated);
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

fn register_write_tool(
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
        Ok(build_diff_preview(
            &vault_path.display_path,
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

fn register_append_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "files.append".to_string(),
        description: format!("Append content to a file. {VAULT_PATH_NOTE}"),
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
        result_mode: ToolResultMode::Inline,
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
        description: format!("Create a new file. {VAULT_PATH_NOTE}"),
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
        result_mode: ToolResultMode::Inline,
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
        description: format!("Edit a file by replacing a line range. {VAULT_PATH_NOTE}"),
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
        result_mode: ToolResultMode::Inline,
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
        Ok(build_diff_preview(
            &vault_path.display_path,
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

struct EditArgs {
    path: String,
    start_line: usize,
    end_line: usize,
    content: String,
}

struct SearchReplaceArgs {
    path: String,
    query: String,
    replace: String,
    literal: bool,
    case_sensitive: bool,
    max_replacements: Option<usize>,
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
    let start_line =
        args.get("start_line")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| ToolError::new("Missing or invalid 'start_line'"))? as usize;
    let end_line =
        args.get("end_line")
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

fn parse_search_replace_args(args: &Value) -> Result<SearchReplaceArgs, ToolError> {
    let path = require_string_arg(args, "path")?;
    let query = require_string_arg(args, "query")?;
    let replace = require_string_arg(args, "replace")?;
    let literal = args
        .get("literal")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let max_replacements = args
        .get("max_replacements")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    if query.is_empty() {
        return Err(ToolError::new("Query cannot be empty"));
    }

    Ok(SearchReplaceArgs {
        path,
        query,
        replace,
        literal,
        case_sensitive,
        max_replacements,
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

fn build_search_replace_regex(edit: &SearchReplaceArgs) -> Result<Regex, ToolError> {
    let pattern = if edit.literal {
        regex::escape(&edit.query)
    } else {
        edit.query.clone()
    };
    let mut builder = RegexBuilder::new(&pattern);
    builder.case_insensitive(!edit.case_sensitive);
    builder
        .build()
        .map_err(|err| ToolError::new(format!("Invalid search pattern: {err}")))
}

fn apply_search_replace(
    original: &str,
    edit: &SearchReplaceArgs,
) -> Result<(String, usize), ToolError> {
    let regex = build_search_replace_regex(edit)?;
    if !edit.literal && regex.is_match("") {
        return Err(ToolError::new(
            "Regex matches empty string; refusing to replace",
        ));
    }

    let replacements = match edit.max_replacements {
        Some(limit) => regex.find_iter(original).take(limit).count(),
        None => regex.find_iter(original).count(),
    };
    if replacements == 0 {
        return Ok((original.to_string(), 0));
    }

    let updated = match edit.max_replacements {
        Some(limit) => regex
            .replacen(original, limit, edit.replace.as_str())
            .into_owned(),
        None => regex
            .replace_all(original, edit.replace.as_str())
            .into_owned(),
    };
    Ok((updated, replacements))
}

fn read_optional_file(path: &Path) -> Result<String, ToolError> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|err| ToolError::new(format!("Failed to read file: {err}")))
}

fn build_diff_preview(path: &str, before: &str, after: &str) -> Value {
    let diff = format!("--- a/{path}\n+++ b/{path}\n@@\n-{before}\n+{after}");
    json!({
        "path": path,
        "before": before,
        "after": after,
        "diff": diff
    })
}

fn list_dir(
    base: &Path,
    current: &Path,
    depth: usize,
    include_files: bool,
    include_dirs: bool,
    entries: &mut Vec<Value>,
) -> Result<(), ToolError> {
    if depth == 0 {
        return Ok(());
    }
    let read_dir = fs::read_dir(current)
        .map_err(|err| ToolError::new(format!("Failed to read directory: {err}")))?;
    for entry in read_dir {
        let entry = entry.map_err(|err| ToolError::new(format!("Failed to read entry: {err}")))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| ToolError::new(format!("Failed to inspect entry: {err}")))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let display_path = crate::tools::vault::to_display_path(base, &path);
        if metadata.is_dir() {
            if include_dirs {
                entries.push(json!({ "path": display_path, "type": "dir" }));
            }
            list_dir(
                base,
                &path,
                depth.saturating_sub(1),
                include_files,
                include_dirs,
                entries,
            )?;
        } else if metadata.is_file() && include_files {
            entries.push(json!({ "path": display_path, "type": "file" }));
        }
    }
    Ok(())
}
