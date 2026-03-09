use crate::db::{Db, ModelOperations};
use crate::tools::vault::resolve_root_path;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use reqwest::blocking::{multipart, Client};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_BETA_FILES: &str = "files-api-2025-04-14";
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const RESULTS_TIMEOUT_SECS: u64 = 300;

fn get_anthropic_api_key(db: &Db) -> Result<String, ToolError> {
    let key = ModelOperations::get_api_key(db, "anthropic")
        .map_err(|e| ToolError::new(format!("DB error reading Anthropic API key: {e}")))?
        .ok_or_else(|| ToolError::new("Anthropic API key is not configured"))?;
    if key.trim().is_empty() {
        return Err(ToolError::new("Anthropic API key is empty"));
    }
    Ok(key)
}

fn anthropic_client(timeout_secs: u64) -> Result<Client, ToolError> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| ToolError::new(format!("Failed to build HTTP client: {e}")))
}

fn extract_anthropic_error(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
    }
    let truncated: String = body.chars().take(300).collect();
    truncated
}

fn validate_id(id: &str, label: &str) -> Result<String, ToolError> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(ToolError::new(format!("{label} is required")));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(ToolError::new(format!("Invalid {label}")));
    }
    Ok(id)
}

fn require_string(args: &Value, field: &str) -> Result<String, ToolError> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing required field: {field}")))
}

pub fn register_anthropic_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    register_file_upload(registry, db.clone())?;
    register_batch_create(registry, db.clone())?;
    register_batch_process_files(registry, db.clone())?;
    register_batch_poll(registry, db.clone())?;
    register_batch_results(registry, db.clone())?;
    register_file_delete(registry, db)?;
    Ok(())
}

fn register_file_upload(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.file_upload".to_string(),
        description: "Upload a local file to the Anthropic Files API. Returns a file_id that can be referenced in batch requests.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to root)"
                },
                "root": {
                    "type": "string",
                    "enum": ["vault", "work"],
                    "description": "Root directory (default: work)"
                },
                "mime_type": {
                    "type": "string",
                    "description": "MIME type override (default: auto-detect from extension)"
                }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "file_id": { "type": "string" },
                "filename": { "type": "string" },
                "size_bytes": { "type": "integer" },
                "created_at": { "type": "string" }
            },
            "required": ["file_id", "filename", "size_bytes", "created_at"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let path_input = require_string(&args, "path")?;
        let root = args
            .get("root")
            .and_then(|v| v.as_str())
            .unwrap_or("work");

        let resolved = resolve_root_path(&db, root, &path_input)?;
        if !resolved.full_path.is_file() {
            return Err(ToolError::new(format!(
                "File not found: {}",
                resolved.display_path
            )));
        }

        let filename = resolved
            .full_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let mime_type = args
            .get("mime_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                mime_guess::from_path(&resolved.full_path)
                    .first_or_octet_stream()
                    .to_string()
            });

        let file_bytes = std::fs::read(&resolved.full_path)
            .map_err(|e| ToolError::new(format!("Failed to read file: {e}")))?;

        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let part = multipart::Part::bytes(file_bytes)
            .file_name(filename.clone())
            .mime_str(&mime_type)
            .map_err(|e| ToolError::new(format!("Invalid MIME type: {e}")))?;
        let form = multipart::Form::new().part("file", part);

        let response = client
            .post(format!("{ANTHROPIC_BASE_URL}/v1/files"))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", ANTHROPIC_BETA_FILES)
            .multipart(form)
            .send()
            .map_err(|e| ToolError::new(format!("Upload request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Files API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body: Value = response
            .json()
            .map_err(|e| ToolError::new(format!("Failed to parse response: {e}")))?;

        Ok(json!({
            "file_id": body.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::new("Anthropic API response missing 'id' field"))?,
            "filename": body.get("filename").and_then(|v| v.as_str()).unwrap_or(&filename),
            "size_bytes": body.get("size_bytes").and_then(|v| v.as_i64()).unwrap_or(0),
            "created_at": body.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_batch_create(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_create".to_string(),
        description: "Create a Message Batch on the Anthropic API. Each request in the batch is a standard Messages API call. Returns a batch_id for tracking.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "requests": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "custom_id": { "type": "string" },
                            "params": {
                                "type": "object",
                                "properties": {
                                    "model": { "type": "string" },
                                    "max_tokens": { "type": "integer" },
                                    "system": { "type": "string" },
                                    "messages": { "type": "array" },
                                    "temperature": { "type": "number" }
                                },
                                "required": ["model", "max_tokens", "messages"]
                            }
                        },
                        "required": ["custom_id", "params"]
                    }
                }
            },
            "required": ["requests"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "status": { "type": "string" },
                "request_counts": {
                    "type": "object",
                    "properties": {
                        "processing": { "type": "integer" },
                        "succeeded": { "type": "integer" },
                        "errored": { "type": "integer" },
                        "canceled": { "type": "integer" },
                        "expired": { "type": "integer" }
                    }
                },
                "created_at": { "type": "string" },
                "expires_at": { "type": "string" }
            },
            "required": ["batch_id", "status"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let requests = args
            .get("requests")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::new("Missing required field: requests"))?;

        if requests.is_empty() {
            return Err(ToolError::new("requests array must not be empty"));
        }

        for (i, req) in requests.iter().enumerate() {
            let custom_id = req
                .get("custom_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if custom_id.trim().is_empty() {
                return Err(ToolError::new(format!(
                    "requests[{i}].custom_id must not be empty"
                )));
            }
        }

        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let body = json!({ "requests": requests });

        let response = client
            .post(format!("{ANTHROPIC_BASE_URL}/v1/messages/batches"))
            .header("Content-Type", "application/json")
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .map_err(|e| ToolError::new(format!("Batch create request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Batch API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body: Value = response
            .json()
            .map_err(|e| ToolError::new(format!("Failed to parse response: {e}")))?;

        Ok(json!({
            "batch_id": body.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::new("Anthropic API response missing 'id' field"))?,
            "status": body.get("processing_status").and_then(|v| v.as_str()).unwrap_or("unknown"),
            "request_counts": body.get("request_counts").cloned().unwrap_or(json!({})),
            "created_at": body.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            "expires_at": body.get("expires_at").and_then(|v| v.as_str()).unwrap_or(""),
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_batch_process_files(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_process_files".to_string(),
        description: "High-level batch tool: takes a list of uploaded file_ids and a shared prompt, constructs one Messages API request per file, and submits them as a batch. Returns a batch_id for tracking. Use this instead of anthropic.batch_create when processing multiple files with the same prompt.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "file_ids": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "file_id": { "type": "string" },
                            "label": {
                                "type": "string",
                                "description": "Human-readable label used as custom_id (e.g. original filename)"
                            }
                        },
                        "required": ["file_id", "label"]
                    },
                    "minItems": 1,
                    "description": "List of Anthropic file_ids with labels (from anthropic.file_upload)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to apply to each file"
                },
                "system": {
                    "type": "string",
                    "description": "Optional system prompt for each request"
                },
                "model": {
                    "type": "string",
                    "description": "Model to use (default: claude-sonnet-4-6)"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Max tokens per response (default: 4096)"
                }
            },
            "required": ["file_ids", "prompt"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "status": { "type": "string" },
                "request_count": { "type": "integer" },
                "request_counts": {
                    "type": "object",
                    "properties": {
                        "processing": { "type": "integer" },
                        "succeeded": { "type": "integer" },
                        "errored": { "type": "integer" },
                        "canceled": { "type": "integer" },
                        "expired": { "type": "integer" }
                    }
                },
                "created_at": { "type": "string" },
                "expires_at": { "type": "string" }
            },
            "required": ["batch_id", "status", "request_count"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let file_entries = args
            .get("file_ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::new("Missing required field: file_ids"))?;

        if file_entries.is_empty() {
            return Err(ToolError::new("file_ids array must not be empty"));
        }

        let prompt = require_string(&args, "prompt")?;
        let system = args.get("system").and_then(|v| v.as_str());
        let model = args
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-6");
        let max_tokens = args
            .get("max_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(4096);

        let mut requests = Vec::with_capacity(file_entries.len());
        for (i, entry) in file_entries.iter().enumerate() {
            let file_id = entry
                .get("file_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::new(format!("file_ids[{i}].file_id is required"))
                })?;
            let label = entry
                .get("label")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::new(format!("file_ids[{i}].label is required"))
                })?;

            validate_id(file_id, &format!("file_ids[{i}].file_id"))?;

            let mut content = vec![
                json!({
                    "type": "document",
                    "source": {
                        "type": "file",
                        "file_id": file_id
                    }
                }),
                json!({
                    "type": "text",
                    "text": prompt
                }),
            ];
            let _ = &mut content; // satisfy borrow checker

            let mut params = json!({
                "model": model,
                "max_tokens": max_tokens,
                "messages": [{
                    "role": "user",
                    "content": content
                }]
            });

            if let Some(system_prompt) = system {
                params["system"] = json!(system_prompt);
            }

            let custom_id: String = label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                .take(64)
                .collect();

            requests.push(json!({
                "custom_id": custom_id,
                "params": params
            }));
        }

        let request_count = requests.len();
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let body = json!({ "requests": requests });

        let response = client
            .post(format!("{ANTHROPIC_BASE_URL}/v1/messages/batches"))
            .header("Content-Type", "application/json")
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", ANTHROPIC_BETA_FILES)
            .json(&body)
            .send()
            .map_err(|e| ToolError::new(format!("Batch create request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Batch API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body: Value = response
            .json()
            .map_err(|e| ToolError::new(format!("Failed to parse response: {e}")))?;

        Ok(json!({
            "batch_id": body.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::new("Anthropic API response missing 'id' field"))?,
            "status": body.get("processing_status").and_then(|v| v.as_str()).unwrap_or("unknown"),
            "request_count": request_count,
            "request_counts": body.get("request_counts").cloned().unwrap_or(json!({})),
            "created_at": body.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            "expires_at": body.get("expires_at").and_then(|v| v.as_str()).unwrap_or(""),
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_batch_poll(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_poll".to_string(),
        description: "Check the status of an Anthropic Message Batch.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" }
            },
            "required": ["batch_id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "status": { "type": "string" },
                "request_counts": {
                    "type": "object",
                    "properties": {
                        "processing": { "type": "integer" },
                        "succeeded": { "type": "integer" },
                        "errored": { "type": "integer" },
                        "canceled": { "type": "integer" },
                        "expired": { "type": "integer" }
                    }
                },
                "created_at": { "type": "string" },
                "expires_at": { "type": "string" },
                "ended_at": { "type": "string" }
            },
            "required": ["batch_id", "status"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let batch_id = validate_id(&require_string(&args, "batch_id")?, "batch_id")?;
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let response = client
            .get(format!(
                "{ANTHROPIC_BASE_URL}/v1/messages/batches/{batch_id}"
            ))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .map_err(|e| ToolError::new(format!("Batch poll request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Batch API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body: Value = response
            .json()
            .map_err(|e| ToolError::new(format!("Failed to parse response: {e}")))?;

        let mut result = json!({
            "batch_id": body.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "status": body.get("processing_status").and_then(|v| v.as_str()).unwrap_or("unknown"),
            "request_counts": body.get("request_counts").cloned().unwrap_or(json!({})),
            "created_at": body.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            "expires_at": body.get("expires_at").and_then(|v| v.as_str()).unwrap_or(""),
        });

        if let Some(ended_at) = body.get("ended_at").and_then(|v| v.as_str()) {
            result["ended_at"] = json!(ended_at);
        }

        Ok(result)
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_batch_results(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.batch_results".to_string(),
        description: "Fetch results of a completed Anthropic Message Batch. Returns JSONL data saved to the specified path or persisted via the tool outputs system.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "save_to": {
                    "type": "string",
                    "description": "Vault-relative path to save results as JSONL. If omitted, results are persisted via the tool outputs system."
                }
            },
            "required": ["batch_id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "batch_id": { "type": "string" },
                "result_count": { "type": "integer" },
                "saved_to": { "type": "string" },
                "results": { "type": "array" }
            },
            "required": ["batch_id", "result_count"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Persist,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let batch_id = validate_id(&require_string(&args, "batch_id")?, "batch_id")?;
        let save_to = args.get("save_to").and_then(|v| v.as_str());
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(RESULTS_TIMEOUT_SECS)?;

        let response = client
            .get(format!(
                "{ANTHROPIC_BASE_URL}/v1/messages/batches/{batch_id}/results"
            ))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .map_err(|e| ToolError::new(format!("Batch results request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Batch API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body = response
            .text()
            .map_err(|e| ToolError::new(format!("Failed to read results body: {e}")))?;

        let mut results: Vec<Value> = Vec::new();
        let mut parse_errors = 0usize;
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(value) => results.push(value),
                Err(_) => parse_errors += 1,
            }
        }
        let result_count = results.len();

        if let Some(save_path) = save_to {
            let resolved = resolve_root_path(&db, "vault", save_path)?;
            if let Some(parent) = resolved.full_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ToolError::new(format!("Failed to create directories: {e}")))?;
            }
            std::fs::write(&resolved.full_path, &body)
                .map_err(|e| ToolError::new(format!("Failed to write results: {e}")))?;

            let mut result = json!({
                "batch_id": batch_id,
                "result_count": result_count,
                "saved_to": resolved.display_path,
            });
            if parse_errors > 0 {
                result["parse_errors"] = json!(parse_errors);
            }
            Ok(result)
        } else {
            let mut result = json!({
                "batch_id": batch_id,
                "result_count": result_count,
                "results": results,
            });
            if parse_errors > 0 {
                result["parse_errors"] = json!(parse_errors);
            }
            Ok(result)
        }
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_file_delete(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "anthropic.file_delete".to_string(),
        description: "Delete a file from the Anthropic Files API.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "file_id": { "type": "string" }
            },
            "required": ["file_id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "file_id": { "type": "string" },
                "deleted": { "type": "boolean" }
            },
            "required": ["file_id", "deleted"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let file_id = validate_id(&require_string(&args, "file_id")?, "file_id")?;
        let api_key = get_anthropic_api_key(&db)?;
        let client = anthropic_client(DEFAULT_TIMEOUT_SECS)?;

        let response = client
            .delete(format!("{ANTHROPIC_BASE_URL}/v1/files/{file_id}"))
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", ANTHROPIC_BETA_FILES)
            .send()
            .map_err(|e| ToolError::new(format!("File delete request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ToolError::new(format!(
                "Anthropic Files API error {status}: {}",
                extract_anthropic_error(&body)
            )));
        }

        let body: Value = response
            .json()
            .map_err(|e| ToolError::new(format!("Failed to parse response: {e}")))?;

        Ok(json!({
            "file_id": body.get("id").and_then(|v| v.as_str()).unwrap_or(&file_id),
            "deleted": body.get("deleted").and_then(|v| v.as_bool()).unwrap_or(true),
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
