use crate::db::Db;
use crate::tools::vault::{ensure_inside_root, get_vault_root, normalize_relative_path, reject_symlink_components};
use crate::tools::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;

const VAULT_PATH_NOTE: &str =
    "Paths are relative to the vault root (use \".\" for root; no absolute paths).";

pub fn register_search_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "search.rg".to_string(),
        description: format!("Search the vault using ripgrep. {VAULT_PATH_NOTE}"),
        args_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "literal": { "type": "boolean" },
                "case_sensitive": { "type": "boolean" },
                "path": { "type": "string" },
                "max_results": { "type": "integer", "minimum": 1 }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "results": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "line": { "type": "integer" },
                            "column": { "type": "integer" },
                            "snippet": { "type": "string" }
                        },
                        "required": ["path", "line", "snippet"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["results"],
            "additionalProperties": false
        }),
        requires_approval: false,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let query = require_string_arg(&args, "query")?;
        let literal = args.get("literal").and_then(|v| v.as_bool()).unwrap_or(false);
        let case_sensitive = args
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as usize;
        let path = args.get("path").and_then(|v| v.as_str()).map(|v| v.to_string());

        let root = get_vault_root(&db)?;
        let search_target = if let Some(path) = path {
            let relative = normalize_relative_path(&path)?;
            reject_symlink_components(&root, &relative)?;
            let full = root.join(&relative);
            ensure_inside_root(&root, &full)?;
            relative.to_string_lossy().to_string()
        } else {
            ".".to_string()
        };

        let mut command = Command::new("rg");
        command
            .current_dir(&root)
            .arg("--json")
            .arg("--with-filename")
            .arg("--line-number")
            .arg("--column")
            .arg("-g")
            .arg("!.obsidian/**")
            .arg("-g")
            .arg("!Attachments/**");

        if !case_sensitive {
            command.arg("-i");
        }
        if literal {
            command.arg("-F");
        }

        command.arg(query);
        command.arg(search_target);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|err| ToolError::new(format!("Failed to run rg: {err}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::new("Failed to read rg output"))?;
        let reader = BufReader::new(stdout);

        let mut results: Vec<Value> = Vec::new();
        let mut reached_limit = false;
        for line in reader.lines() {
            let line = line.map_err(|err| ToolError::new(format!("Failed to read rg output: {err}")))?;
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if value.get("type").and_then(|v| v.as_str()) != Some("match") {
                continue;
            }

            if let Some(data) = value.get("data") {
                let path = data
                    .get("path")
                    .and_then(|p| p.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let line_number = data
                    .get("line_number")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as i64;
                let snippet = data
                    .get("lines")
                    .and_then(|v| v.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim_end_matches('\n')
                    .to_string();
                let column = data
                    .get("submatches")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.get("start"))
                    .and_then(|v| v.as_u64())
                    .map(|col| col as i64 + 1);

                let mut entry = json!({
                    "path": path,
                    "line": line_number,
                    "snippet": snippet
                });
                if let Some(column) = column {
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert("column".to_string(), json!(column));
                    }
                }

                results.push(entry);
                if results.len() >= max_results {
                    reached_limit = true;
                    break;
                }
            }
        }

        if reached_limit {
            let _ = child.kill();
        }

        let status = child
            .wait()
            .map_err(|err| ToolError::new(format!("Failed to wait for rg: {err}")))?;
        if let Some(code) = status.code() {
            if code != 0 && code != 1 {
                return Err(ToolError::new(format!("rg exited with status {code}")));
            }
        }

        Ok(json!({ "results": results }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn require_string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing or invalid '{key}'")))
}
