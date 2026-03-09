use crate::db::Db;
use crate::tools::vault::{get_vault_root, get_work_root};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_OUTPUT_BYTES: usize = 256 * 1024; // 256 KB per stream

pub fn register_shell_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "shell.exec".to_string(),
        description: "Execute a shell command and return its output. Use for file operations like \
            splitting files (split), counting lines (wc -l), listing files (ls), etc. \
            Use root=\"vault\" or root=\"work\" to run in the configured vault/work directory. \
            Always requires user approval before running."
            .to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
                "args": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "root": { "type": "string", "enum": ["vault", "work"] },
                "working_dir": { "type": "string" },
                "timeout_ms": { "type": "integer", "minimum": 1 }
            },
            "required": ["command"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "stdout":    { "type": "string" },
                "stderr":    { "type": "string" },
                "exit_code": { "type": "integer" },
                "success":   { "type": "boolean" },
                "truncated": { "type": "boolean" }
            },
            "required": ["stdout", "stderr", "exit_code", "success", "truncated"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let command = require_string_arg(&args, "command")?;
        let cmd_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let root = args.get("root").and_then(|v| v.as_str());
        let working_dir_arg = args
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let resolved_dir: Option<PathBuf> = match root {
            Some("vault") => Some(get_vault_root(&db)?),
            Some("work") => Some(get_work_root(&db)?),
            Some(other) => {
                return Err(ToolError::new(format!(
                    "Invalid root '{other}'; expected 'vault' or 'work'"
                )))
            }
            None => working_dir_arg.map(PathBuf::from),
        };

        if let Some(ref dir) = resolved_dir {
            if !dir.is_dir() {
                return Err(ToolError::new(format!(
                    "working_dir does not exist or is not a directory: {}",
                    dir.display()
                )));
            }
        }

        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        let mut cmd = Command::new(&command);
        cmd.args(&cmd_args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        if let Some(ref dir) = resolved_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .map_err(|err| ToolError::new(format!("Failed to spawn '{command}': {err}")))?;

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let (tx, rx) = mpsc::channel::<Result<(i32, String, String, bool), String>>();

        std::thread::spawn(move || {
            let mut stdout_bytes = Vec::new();
            let mut stderr_bytes = Vec::new();
            let mut truncated = false;

            if let Some(ref mut out) = stdout_handle {
                let mut buf = vec![0u8; MAX_OUTPUT_BYTES + 1];
                let n = out.read(&mut buf).unwrap_or(0);
                if n > MAX_OUTPUT_BYTES {
                    stdout_bytes.extend_from_slice(&buf[..MAX_OUTPUT_BYTES]);
                    truncated = true;
                } else {
                    stdout_bytes.extend_from_slice(&buf[..n]);
                    // drain the rest so the process can exit
                    let _ = std::io::copy(out, &mut std::io::sink());
                }
            }
            if let Some(ref mut err) = stderr_handle {
                let mut buf = vec![0u8; MAX_OUTPUT_BYTES + 1];
                let n = err.read(&mut buf).unwrap_or(0);
                if n > MAX_OUTPUT_BYTES {
                    stderr_bytes.extend_from_slice(&buf[..MAX_OUTPUT_BYTES]);
                    truncated = true;
                } else {
                    stderr_bytes.extend_from_slice(&buf[..n]);
                    let _ = std::io::copy(err, &mut std::io::sink());
                }
            }

            let exit_code = match child.wait() {
                Ok(status) => status.code().unwrap_or(-1),
                Err(err) => {
                    let _ = tx.send(Err(format!("Failed to wait for process: {err}")));
                    return;
                }
            };

            let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
            let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
            let _ = tx.send(Ok((exit_code, stdout, stderr, truncated)));
        });

        match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
            Ok(Ok((exit_code, stdout, stderr, truncated))) => Ok(json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": exit_code,
                "success": exit_code == 0,
                "truncated": truncated
            })),
            Ok(Err(err)) => Err(ToolError::new(err)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // The thread still owns the child, but we can't kill it from here.
                // Return a timeout result; the child will be dropped when the thread finishes.
                Ok(json!({
                    "stdout": "",
                    "stderr": format!("Command timed out after {timeout_ms}ms"),
                    "exit_code": -1,
                    "success": false,
                    "truncated": false
                }))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(ToolError::new("Command thread disconnected unexpectedly"))
            }
        }
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn require_string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing or invalid '{key}'")))
}
