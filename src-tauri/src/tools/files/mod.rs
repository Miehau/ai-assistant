use crate::db::Db;
use crate::tools::{ToolError, ToolRegistry};
use regex::{Regex, RegexBuilder};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

mod append;
mod create;
mod edit;
mod list;
mod read;
mod read_range;
mod search_replace;
mod write;

pub(crate) const VAULT_PATH_NOTE: &str =
    "Paths are relative to the selected root (default vault; use root=\"work\" for work directory; use \".\" for root; no absolute paths).";
pub(crate) const DEFAULT_READ_MAX_LINES: usize = 200;
pub(crate) const DEFAULT_READ_MAX_CHARS: usize = 20_000;
pub(crate) const MAX_READ_FILE_SIZE: u64 = 64 * 1024;
pub(crate) const MAX_READ_IMAGE_SIZE: u64 = 5 * 1024 * 1024; // 5MB

pub fn register_file_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    list::register_list_tool(registry, db.clone())?;
    read::register_read_tool(registry, db.clone(), "files.read", "Read file contents")?;
    read::register_read_tool(registry, db.clone(), "files.open", "Open file contents")?;
    read_range::register_read_range_tool(registry, db.clone())?;
    search_replace::register_search_replace_tool(registry, db.clone())?;
    write::register_write_tool(
        registry,
        db.clone(),
        "files.write",
        "Write/replace file contents",
    )?;
    write::register_write_tool(
        registry,
        db.clone(),
        "files.replace",
        "Replace file contents",
    )?;
    append::register_append_tool(registry, db.clone())?;
    create::register_create_tool(registry, db.clone())?;
    edit::register_edit_tool(registry, db)?;
    Ok(())
}

pub(crate) struct EditArgs {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

pub(crate) struct SearchReplaceArgs {
    pub path: String,
    pub query: String,
    pub replace: String,
    pub literal: bool,
    pub case_sensitive: bool,
    pub max_replacements: Option<usize>,
}

pub(crate) fn require_string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing or invalid '{key}'")))
}

pub(crate) fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

pub(crate) fn parse_root_arg(args: &Value) -> Result<&str, ToolError> {
    let root = args
        .get("root")
        .and_then(|value| value.as_str())
        .unwrap_or("vault");
    match root {
        "vault" | "work" => Ok(root),
        _ => Err(ToolError::new("Invalid root; expected 'vault' or 'work'")),
    }
}

pub(crate) fn detect_media_type(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "png" => Some("image/png".to_string()),
        "gif" => Some("image/gif".to_string()),
        "webp" => Some("image/webp".to_string()),
        "bmp" => Some("image/bmp".to_string()),
        "svg" => Some("image/svg+xml".to_string()),
        "ico" => Some("image/x-icon".to_string()),
        _ => None,
    }
}

pub(crate) fn parse_path_content_args(args: &Value) -> Result<(String, String), ToolError> {
    let path = require_string_arg(args, "path")?;
    let content = require_string_arg(args, "content")?;
    Ok((path, content))
}

pub(crate) fn parse_edit_args(args: &Value) -> Result<EditArgs, ToolError> {
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

pub(crate) fn parse_search_replace_args(args: &Value) -> Result<SearchReplaceArgs, ToolError> {
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

pub(crate) fn apply_line_edit(
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

pub(crate) fn build_search_replace_regex(edit: &SearchReplaceArgs) -> Result<Regex, ToolError> {
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

pub(crate) fn apply_search_replace(
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

pub(crate) fn read_optional_file(path: &Path) -> Result<String, ToolError> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|err| ToolError::new(format!("Failed to read file: {err}")))
}

pub(crate) fn build_diff_preview(path: &str, before: &str, after: &str) -> Value {
    let diff = format!("--- a/{path}\n+++ b/{path}\n@@\n-{before}\n+{after}");
    json!({
        "path": path,
        "before": before,
        "after": after,
        "diff": diff
    })
}

pub(crate) fn list_dir(
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
