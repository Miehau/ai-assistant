use crate::db::{Db, PreferenceOperations};
use crate::tools::vault::resolve_vault_path;
use crate::tools::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::redirect::Policy;
use reqwest::Method;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

const PREF_ALLOWED_HOSTS: &str = "plugins.web.allowed_hosts";
const DEFAULT_MAX_BYTES: usize = 200_000;
const DEFAULT_MAX_DOWNLOAD_BYTES: usize = 10_485_760; // 10 MB
const DEFAULT_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_USER_AGENT: &str = "ai-agent/1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AllowedHost {
    host: String,
    allow_private: bool,
    approved_at: i64,
}

pub fn register_web_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    register_approve_tool(registry, db.clone())?;
    register_fetch_tool(registry, db.clone())?;
    register_request_tool(registry, db.clone())?;
    register_download_tool(registry, db)?;
    Ok(())
}

fn register_approve_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.approve_domain".to_string(),
        description: "Approve a website host for automated access (exact host).".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "allow_private": { "type": "boolean" }
            },
            "required": ["url"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "host": { "type": "string" },
                "allow_private": { "type": "boolean" },
                "saved": { "type": "boolean" }
            },
            "required": ["host", "allow_private", "saved"],
            "additionalProperties": false
        }),
        requires_approval: false,
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let url = require_string_arg(&args, "url")?;
        let allow_private = args
            .get("allow_private")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let host = normalize_host_from_input(&url)?;

        if is_private_host(&host) && !allow_private {
            return Err(ToolError::new(
                "Private/local hosts require allow_private=true",
            ));
        }

        let mut allowed = load_allowlist(&handler_db)?;
        let now = chrono::Utc::now().timestamp();
        if let Some(entry) = allowed.iter_mut().find(|entry| entry.host == host) {
            entry.allow_private = allow_private;
            entry.approved_at = now;
        } else {
            allowed.push(AllowedHost {
                host: host.clone(),
                allow_private,
                approved_at: now,
            });
        }
        save_allowlist(&handler_db, &allowed)?;

        Ok(json!({
            "host": host,
            "allow_private": allow_private,
            "saved": true
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let url = require_string_arg(&args, "url")?;
        let allow_private = args
            .get("allow_private")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let host = normalize_host_from_input(&url)?;
        let host_for_preview = host.clone();
        let mut preview = json!({
            "host": host_for_preview,
            "allow_private": allow_private
        });
        if let Ok(existing) = load_allowlist(&preview_db) {
            if let Some(entry) = existing.iter().find(|entry| entry.host == host) {
                if let Some(obj) = preview.as_object_mut() {
                    obj.insert(
                        "existing".to_string(),
                        json!({ "allow_private": entry.allow_private, "approved_at": entry.approved_at }),
                    );
                }
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

fn register_fetch_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.fetch".to_string(),
        description: "Fetch a web page and extract text plus links (host must be approved).".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_bytes": { "type": "integer", "minimum": 1 },
                "timeout_ms": { "type": "integer", "minimum": 1 },
                "user_agent": { "type": "string" },
                "same_host_only": { "type": "boolean" },
                "extract_links": { "type": "boolean" },
                "include_html": { "type": "boolean" },
                "max_links": { "type": "integer", "minimum": 1 }
            },
            "required": ["url"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "status": { "type": "integer" },
                "content_type": { "type": "string" },
                "title": { "type": "string" },
                "text": { "type": "string" },
                "html": { "type": "string" },
                "links": { "type": "array", "items": { "type": "string" } },
                "truncated": { "type": "boolean" },
                "bytes": { "type": "integer" }
            },
            "required": ["url", "status", "content_type", "text", "links", "truncated", "bytes"],
            "additionalProperties": false
        }),
        requires_approval: false,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let url = require_string_arg(&args, "url")?;
        let max_bytes = args
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_BYTES as u64) as usize;
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let user_agent = args
            .get("user_agent")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_USER_AGENT);
        let same_host_only = args
            .get("same_host_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let extract_links = args
            .get("extract_links")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let include_html = args
            .get("include_html")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_links = args
            .get("max_links")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as usize;

        let parsed = parse_url(&url)?;
        let original_host = normalize_host(
            parsed
                .host_str()
                .ok_or_else(|| ToolError::new("URL missing host"))?,
        )?;

        let allowlist = load_allowlist(&db)?;
        ensure_host_allowed(&allowlist, &original_host)?;

        let allowed_map = build_allowlist_map(&allowlist);
        let client = build_client(timeout_ms, user_agent, &allowed_map, Some(&original_host), same_host_only)?;

        let response = client
            .get(parsed.as_str())
            .send()
            .map_err(|err| ToolError::new(format!("Request failed: {err}")))?;

        if response.status().is_redirection() {
            return Err(ToolError::new("Redirect blocked by host policy"));
        }

        let base_url = response.url().clone();
        let final_url = response.url().to_string();
        let final_host = response
            .url()
            .host_str()
            .map(|host| normalize_host(host))
            .transpose()?
            .unwrap_or_else(|| original_host.clone());

        ensure_host_allowed(&allowlist, &final_host)?;
        if same_host_only && final_host != original_host {
            return Err(ToolError::new("Redirected to a different host"));
        }

        let status = response.status().as_u16() as i64;
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let (body, truncated) = read_limited_body(response, max_bytes)?;
        let body_text = String::from_utf8_lossy(&body).to_string();
        let bytes = body.len() as i64;

        let mut title = String::new();
        let mut text = String::new();
        let mut links: Vec<String> = Vec::new();

        if is_html_content(&content_type, &body_text) {
            let document = Html::parse_document(&body_text);
            title = extract_title(&document);
            text = extract_text(&document);
            if extract_links {
                links = extract_links_from_document(&document, &base_url, same_host_only, max_links);
            }
        } else if is_text_content(&content_type) {
            text = body_text.clone();
        }

        let mut result = json!({
            "url": final_url,
            "status": status,
            "content_type": content_type,
            "title": title,
            "text": text,
            "links": links,
            "truncated": truncated,
            "bytes": bytes
        });

        if include_html {
            if let Some(obj) = result.as_object_mut() {
                obj.insert("html".to_string(), json!(body_text));
            }
        }

        Ok(result)
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_request_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.request".to_string(),
        description: "Send an HTTP request and return the response (host must be approved).".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "method": { "type": "string" },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                },
                "body": { "type": "string" },
                "json": {},
                "max_bytes": { "type": "integer", "minimum": 1 },
                "timeout_ms": { "type": "integer", "minimum": 1 },
                "user_agent": { "type": "string" },
                "same_host_only": { "type": "boolean" }
            },
            "required": ["url"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "method": { "type": "string" },
                "status": { "type": "integer" },
                "content_type": { "type": "string" },
                "text": { "type": "string" },
                "json": {},
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                },
                "truncated": { "type": "boolean" },
                "bytes": { "type": "integer" }
            },
            "required": ["url", "method", "status", "content_type", "text", "truncated", "bytes"],
            "additionalProperties": false
        }),
        requires_approval: true,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let url = require_string_arg(&args, "url")?;
        let method_raw = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");
        let method = parse_method(method_raw)?;
        let headers = parse_headers(&args)?;
        let body = args.get("body").and_then(|v| v.as_str()).map(|v| v.to_string());
        let json_body = args.get("json").filter(|v| !v.is_null()).cloned();
        if body.is_some() && json_body.is_some() {
            return Err(ToolError::new("Provide either 'body' or 'json', not both"));
        }

        let max_bytes = args
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_BYTES as u64) as usize;
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let user_agent = args
            .get("user_agent")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_USER_AGENT);
        let same_host_only = args
            .get("same_host_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let parsed = parse_url(&url)?;
        let original_host = normalize_host(
            parsed
                .host_str()
                .ok_or_else(|| ToolError::new("URL missing host"))?,
        )?;

        let allowlist = load_allowlist(&db)?;
        ensure_host_allowed(&allowlist, &original_host)?;

        let allowed_map = build_allowlist_map(&allowlist);
        let client = build_client(
            timeout_ms,
            user_agent,
            &allowed_map,
            Some(&original_host),
            same_host_only,
        )?;

        let mut request = client.request(method.clone(), parsed.as_str());
        if !headers.is_empty() {
            request = request.headers(headers);
        }
        if let Some(json_body) = json_body {
            request = request.json(&json_body);
        } else if let Some(body) = body {
            request = request.body(body);
        }

        let response = request
            .send()
            .map_err(|err| ToolError::new(format!("Request failed: {err}")))?;

        if response.status().is_redirection() {
            return Err(ToolError::new("Redirect blocked by host policy"));
        }

        let final_url = response.url().to_string();
        let final_host = response
            .url()
            .host_str()
            .map(|host| normalize_host(host))
            .transpose()?
            .unwrap_or_else(|| original_host.clone());
        ensure_host_allowed(&allowlist, &final_host)?;
        if same_host_only && final_host != original_host {
            return Err(ToolError::new("Redirected to a different host"));
        }

        let status = response.status().as_u16() as i64;
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let headers_json = headers_to_json(response.headers());

        let (body_bytes, truncated) = read_limited_body(response, max_bytes)?;
        let bytes = body_bytes.len() as i64;
        let text = if is_text_content(&content_type) {
            String::from_utf8_lossy(&body_bytes).to_string()
        } else {
            String::new()
        };

        let mut result = json!({
            "url": final_url,
            "method": method.as_str(),
            "status": status,
            "content_type": content_type,
            "text": text,
            "headers": headers_json,
            "truncated": truncated,
            "bytes": bytes
        });

        if is_text_content(&content_type) && content_type.to_ascii_lowercase().contains("json") {
            if let Ok(parsed_json) = serde_json::from_str::<Value>(&text) {
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("json".to_string(), parsed_json);
                }
            }
        }

        Ok(result)
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_download_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.download".to_string(),
        description: "Download one or more URLs into the vault attachments folder (host must be approved).".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "urls": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                "base_url": { "type": "string" },
                "vault_path": { "type": "string" },
                "max_bytes_per_file": { "type": "integer", "minimum": 1 },
                "timeout_ms": { "type": "integer", "minimum": 1 },
                "user_agent": { "type": "string" },
                "same_host_only": { "type": "boolean" },
                "flatten": { "type": "boolean" },
                "rename_strategy": { "type": "string", "enum": ["safe", "overwrite"] }
            },
            "required": ["urls"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "base_path": { "type": "string" },
                "results": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "url": { "type": "string" },
                            "success": { "type": "boolean" },
                            "status": { "type": "integer" },
                            "content_type": { "type": "string" },
                            "bytes_written": { "type": "integer" },
                            "path": { "type": "string" },
                            "error": { "type": "string" }
                        },
                        "required": ["url", "success"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["base_path", "results"],
            "additionalProperties": false
        }),
        requires_approval: false,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let urls = args
            .get("urls")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::new("Missing or invalid 'urls'"))?;
        let base_url = args.get("base_url").and_then(|v| v.as_str());
        let vault_path = args
            .get("vault_path")
            .and_then(|v| v.as_str())
            .unwrap_or("Attachments");
        let max_bytes = args
            .get("max_bytes_per_file")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_DOWNLOAD_BYTES as u64) as usize;
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let user_agent = args
            .get("user_agent")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_USER_AGENT);
        let same_host_only = args
            .get("same_host_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let flatten = args
            .get("flatten")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let rename_strategy = args
            .get("rename_strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("safe");

        let allowlist = load_allowlist(&db)?;
        let allowed_map = build_allowlist_map(&allowlist);

        let base_url_parsed = match base_url {
            Some(url) => Some(parse_url(url)?),
            None => None,
        };
        let base_host = if same_host_only {
            if let Some(base) = &base_url_parsed {
                normalize_host(
                    base.host_str()
                        .ok_or_else(|| ToolError::new("base_url missing host"))?,
                )?
            } else {
                let first = urls
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::new("urls must be non-empty strings"))?;
                let parsed = parse_url(first)?;
                normalize_host(
                    parsed
                        .host_str()
                        .ok_or_else(|| ToolError::new("URL missing host"))?,
                )?
            }
        } else {
            String::new()
        };

        if same_host_only {
            ensure_host_allowed(&allowlist, &base_host)?;
        }

        let base_dir = resolve_vault_path(&db, vault_path)?;
        if base_dir.full_path.exists() && !base_dir.full_path.is_dir() {
            return Err(ToolError::new("vault_path must be a directory"));
        }
        std::fs::create_dir_all(&base_dir.full_path)
            .map_err(|err| ToolError::new(format!("Failed to create vault directory: {err}")))?;

        let client = build_client(timeout_ms, user_agent, &allowed_map, Some(&base_host), same_host_only)?;

        let mut results = Vec::new();
        for url_value in urls {
            let url_str = match url_value.as_str() {
                Some(value) => value,
                None => {
                    results.push(json!({
                        "url": "",
                        "success": false,
                        "error": "Invalid URL value"
                    }));
                    continue;
                }
            };

            let resolved = match resolve_download_url(url_str, base_url_parsed.as_ref()) {
                Ok(url) => url,
                Err(err) => {
                    results.push(json!({
                        "url": url_str,
                        "success": false,
                        "error": err.message
                    }));
                    continue;
                }
            };

            let host = match resolved.host_str() {
                Some(host) => match normalize_host(host) {
                    Ok(host) => host,
                    Err(err) => {
                        results.push(json!({
                            "url": url_str,
                            "success": false,
                            "error": err.message
                        }));
                        continue;
                    }
                },
                None => {
                    results.push(json!({
                        "url": url_str,
                        "success": false,
                        "error": "URL missing host"
                    }));
                    continue;
                }
            };

            if same_host_only && host != base_host {
                results.push(json!({
                    "url": url_str,
                    "success": false,
                    "error": "URL host does not match base host"
                }));
                continue;
            }

            if let Err(err) = ensure_host_allowed(&allowlist, &host) {
                results.push(json!({
                    "url": url_str,
                    "success": false,
                    "error": err.message
                }));
                continue;
            }

            let response = match client.get(resolved.as_str()).send() {
                Ok(response) => response,
                Err(err) => {
                    results.push(json!({
                        "url": url_str,
                        "success": false,
                        "error": format!("Request failed: {err}")
                    }));
                    continue;
                }
            };

            if response.status().is_redirection() {
                results.push(json!({
                    "url": url_str,
                    "success": false,
                    "error": "Redirect blocked by host policy"
                }));
                continue;
            }

            let status = response.status().as_u16() as i64;
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            let (body, truncated) = match read_limited_body(response, max_bytes) {
                Ok(result) => result,
                Err(err) => {
                    results.push(json!({
                        "url": url_str,
                        "success": false,
                        "status": status,
                        "content_type": content_type,
                        "error": err.message
                    }));
                    continue;
                }
            };

            if truncated {
                results.push(json!({
                    "url": url_str,
                    "success": false,
                    "status": status,
                    "content_type": content_type,
                    "error": "Download exceeded max_bytes_per_file"
                }));
                continue;
            }

            let (relative_path, display_path) = match build_download_path(
                &db,
                &base_dir,
                &resolved,
                &content_type,
                flatten,
                rename_strategy,
            ) {
                Ok(path) => path,
                Err(err) => {
                    results.push(json!({
                        "url": url_str,
                        "success": false,
                        "status": status,
                        "content_type": content_type,
                        "error": err.message
                    }));
                    continue;
                }
            };

            if let Err(err) = std::fs::write(&relative_path, &body) {
                results.push(json!({
                    "url": url_str,
                    "success": false,
                    "status": status,
                    "content_type": content_type,
                    "error": format!("Failed to write file: {err}")
                }));
                continue;
            }

            results.push(json!({
                "url": url_str,
                "success": true,
                "status": status,
                "content_type": content_type,
                "bytes_written": body.len() as i64,
                "path": display_path
            }));
        }

        let base_display = if base_dir.display_path.is_empty() {
            ".".to_string()
        } else {
            base_dir.display_path
        };

        Ok(json!({
            "base_path": base_display,
            "results": results
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn load_allowlist(db: &Db) -> Result<Vec<AllowedHost>, ToolError> {
    let raw = PreferenceOperations::get_preference(db, PREF_ALLOWED_HOSTS)
        .map_err(|err| ToolError::new(format!("Failed to load web allowlist: {err}")))?;
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&raw).map_err(|err| ToolError::new(format!("Invalid allowlist: {err}")))
}

fn save_allowlist(db: &Db, list: &[AllowedHost]) -> Result<(), ToolError> {
    let value = serde_json::to_string(list)
        .map_err(|err| ToolError::new(format!("Failed to serialize allowlist: {err}")))?;
    PreferenceOperations::set_preference(db, PREF_ALLOWED_HOSTS, &value)
        .map_err(|err| ToolError::new(format!("Failed to save web allowlist: {err}")))?;
    Ok(())
}

fn build_allowlist_map(list: &[AllowedHost]) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    for entry in list {
        map.insert(entry.host.clone(), entry.allow_private);
    }
    map
}

fn parse_method(input: &str) -> Result<Method, ToolError> {
    let normalized = input.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(ToolError::new("Method cannot be empty"));
    }
    Method::from_bytes(normalized.as_bytes())
        .map_err(|_| ToolError::new(format!("Invalid method '{input}'")))
}

fn parse_headers(args: &Value) -> Result<HeaderMap, ToolError> {
    let Some(value) = args.get("headers") else {
        return Ok(HeaderMap::new());
    };
    if value.is_null() {
        return Ok(HeaderMap::new());
    }
    let obj = value
        .as_object()
        .ok_or_else(|| ToolError::new("Invalid 'headers' (expected object)"))?;
    let mut headers = HeaderMap::new();
    for (key, raw_value) in obj {
        let value = raw_value
            .as_str()
            .ok_or_else(|| ToolError::new(format!("Invalid header value for '{key}'")))?;
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| ToolError::new(format!("Invalid header name '{key}'")))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| ToolError::new(format!("Invalid header value for '{key}'")))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn headers_to_json(headers: &HeaderMap) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        if let Ok(text) = value.to_str() {
            map.insert(name.to_string(), json!(text));
        }
    }
    Value::Object(map)
}

fn normalize_host_from_input(input: &str) -> Result<String, ToolError> {
    let url = parse_url(input)?;
    let host = url.host_str().ok_or_else(|| ToolError::new("URL missing host"))?;
    normalize_host(host)
}

fn normalize_host(host: &str) -> Result<String, ToolError> {
    let normalized = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(ToolError::new("Host is empty"));
    }
    Ok(normalized)
}

fn parse_url(input: &str) -> Result<Url, ToolError> {
    match Url::parse(input) {
        Ok(url) => Ok(url),
        Err(_) => {
            let with_scheme = format!("https://{input}");
            Url::parse(&with_scheme)
                .map_err(|err| ToolError::new(format!("Invalid URL: {err}")))
        }
    }
}

fn ensure_host_allowed(list: &[AllowedHost], host: &str) -> Result<(), ToolError> {
    let entry = list.iter().find(|entry| entry.host == host);
    let Some(entry) = entry else {
        return Err(ToolError::new(format!(
            "Host not approved: {host}. Use web.approve_domain first."
        )));
    };
    if is_private_host(host) && !entry.allow_private {
        return Err(ToolError::new(
            "Private/local host blocked. Re-approve with allow_private=true.",
        ));
    }
    Ok(())
}

fn is_private_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_ip(ip);
    }
    false
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => {
            addr.is_private()
                || addr.is_loopback()
                || addr.is_link_local()
                || addr.is_unspecified()
                || addr.is_multicast()
                || addr.is_broadcast()
        }
        IpAddr::V6(addr) => {
            addr.is_loopback()
                || addr.is_unique_local()
                || addr.is_unicast_link_local()
                || addr.is_unspecified()
                || addr.is_multicast()
        }
    }
}

fn build_client(
    timeout_ms: u64,
    user_agent: &str,
    allowlist: &HashMap<String, bool>,
    base_host: Option<&str>,
    same_host_only: bool,
) -> Result<Client, ToolError> {
    let allowlist = allowlist.clone();
    let base_host = base_host.map(|host| host.to_string());
    let policy = Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.stop();
        }
        let host = attempt.url().host_str().unwrap_or("");
        let host = match normalize_host(host) {
            Ok(host) => host,
            Err(_) => return attempt.stop(),
        };
        if same_host_only {
            if let Some(base) = &base_host {
                if &host != base {
                    return attempt.stop();
                }
            }
        }
        match allowlist.get(&host) {
            Some(allow_private) => {
                if is_private_host(&host) && !*allow_private {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }
            None => attempt.stop(),
        }
    });

    Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .user_agent(user_agent)
        .redirect(policy)
        .build()
        .map_err(|err| ToolError::new(format!("Failed to build client: {err}")))
}

fn read_limited_body(response: reqwest::blocking::Response, max_bytes: usize) -> Result<(Vec<u8>, bool), ToolError> {
    use std::io::Read;
    let mut body = Vec::new();
    let mut limited = response.take((max_bytes + 1) as u64);
    limited
        .read_to_end(&mut body)
        .map_err(|err| ToolError::new(format!("Failed to read response: {err}")))?;
    let truncated = body.len() > max_bytes;
    if truncated {
        body.truncate(max_bytes);
    }
    Ok((body, truncated))
}

fn is_html_content(content_type: &str, body: &str) -> bool {
    content_type.to_ascii_lowercase().contains("text/html") || body.contains("<html")
}

fn is_text_content(content_type: &str) -> bool {
    let lower = content_type.to_ascii_lowercase();
    lower.starts_with("text/") || lower.contains("json") || lower.contains("xml")
}

fn extract_title(document: &Html) -> String {
    let selector = match Selector::parse("title") {
        Ok(selector) => selector,
        Err(_) => return String::new(),
    };
    document
        .select(&selector)
        .next()
        .map(|node| node.text().collect::<Vec<_>>().join(" ").trim().to_string())
        .unwrap_or_default()
}

fn extract_text(document: &Html) -> String {
    let selector = Selector::parse("body").ok();
    let text_iter = if let Some(selector) = selector {
        document
            .select(&selector)
            .next()
            .map(|node| node.text().collect::<Vec<_>>())
            .unwrap_or_default()
    } else {
        document.root_element().text().collect::<Vec<_>>()
    };

    let mut result = String::new();
    for chunk in text_iter {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(trimmed);
    }
    result
}

fn extract_links_from_document(
    document: &Html,
    base_url: &Url,
    same_host_only: bool,
    max_links: usize,
) -> Vec<String> {
    let selector = match Selector::parse("a[href]") {
        Ok(selector) => selector,
        Err(_) => return Vec::new(),
    };
    let mut seen = HashSet::new();
    let base_host = base_url.host_str().unwrap_or("").to_ascii_lowercase();
    let mut links = Vec::new();

    for node in document.select(&selector) {
        if links.len() >= max_links {
            break;
        }
        let href = match node.value().attr("href") {
            Some(href) => href.trim(),
            None => continue,
        };
        if href.is_empty() {
            continue;
        }
        let resolved = match base_url.join(href) {
            Ok(url) => url,
            Err(_) => continue,
        };
        let scheme = resolved.scheme();
        if scheme != "http" && scheme != "https" {
            continue;
        }
        let host = resolved.host_str().unwrap_or("").to_ascii_lowercase();
        if same_host_only && host != base_host {
            continue;
        }
        let link = resolved.to_string();
        if seen.insert(link.clone()) {
            links.push(link);
        }
    }
    links
}

fn resolve_download_url(input: &str, base_url: Option<&Url>) -> Result<Url, ToolError> {
    match Url::parse(input) {
        Ok(url) => Ok(url),
        Err(_) => {
            let Some(base) = base_url else {
                return Err(ToolError::new("Relative URL requires base_url"));
            };
            base.join(input)
                .map_err(|err| ToolError::new(format!("Invalid URL: {err}")))
        }
    }
}

fn build_download_path(
    db: &Db,
    base_dir: &crate::tools::vault::VaultPath,
    url: &Url,
    content_type: &str,
    flatten: bool,
    rename_strategy: &str,
) -> Result<(PathBuf, String), ToolError> {
    let mut subdir = String::new();
    if !flatten {
        if let Some(segments) = url.path_segments() {
            let parts = segments
                .filter(|segment| !segment.is_empty())
                .map(|segment| sanitize_segment(segment))
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            if parts.len() > 1 {
                subdir = parts[..parts.len() - 1].join("/");
            }
        }
    }

    let mut filename = file_name_from_url(url);
    if !has_extension(&filename) {
        if let Some(extension) = extension_from_content_type(content_type) {
            filename = format!("{filename}.{extension}");
        }
    }
    filename = sanitize_segment(&filename);
    if filename.is_empty() {
        filename = "download".to_string();
    }

    let relative = if base_dir.display_path.is_empty() {
        if subdir.is_empty() {
            filename.clone()
        } else {
            format!("{}/{}", subdir, filename)
        }
    } else if subdir.is_empty() {
        format!("{}/{}", base_dir.display_path, filename)
    } else {
        format!("{}/{}/{}", base_dir.display_path, subdir, filename)
    };

    let mut resolved = resolve_vault_path(db, &relative)?;
    if rename_strategy == "safe" {
        resolved = ensure_unique_path(resolved)?;
    }

    if let Some(parent) = resolved.full_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| ToolError::new(format!("Failed to create directories: {err}")))?;
    }

    Ok((resolved.full_path, resolved.display_path))
}

fn ensure_unique_path(
    path: crate::tools::vault::VaultPath,
) -> Result<crate::tools::vault::VaultPath, ToolError> {
    if !path.full_path.exists() {
        return Ok(path);
    }
    let mut counter = 1;
    let base = path.full_path.clone();
    let file_name = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let (stem, ext) = split_extension(file_name);
    loop {
        let candidate = if ext.is_empty() {
            format!("{stem}-{counter}")
        } else {
            format!("{stem}-{counter}.{ext}")
        };
        let mut candidate_path = base.clone();
        candidate_path.set_file_name(candidate);
        if !candidate_path.exists() {
            let display = path.display_path.clone();
            let display_candidate = if ext.is_empty() {
                format!("{stem}-{counter}")
            } else {
                format!("{stem}-{counter}.{ext}")
            };
            let display_base = PathBuf::from(display);
            let display_path = match display_base.parent() {
                Some(parent) => parent.join(&display_candidate),
                None => PathBuf::from(display_candidate.clone()),
            };
            let display_path = display_path.to_string_lossy().replace('\\', "/");
            return Ok(crate::tools::vault::VaultPath {
                full_path: candidate_path,
                display_path,
            });
        }
        counter += 1;
        if counter > 500 {
            return Err(ToolError::new("Failed to generate unique filename"));
        }
    }
}

fn file_name_from_url(url: &Url) -> String {
    if let Some(mut segments) = url.path_segments() {
        if let Some(last) = segments.next_back() {
            if !last.is_empty() {
                return last.to_string();
            }
        }
    }
    "download".to_string()
}

fn has_extension(name: &str) -> bool {
    PathBuf::from(name).extension().is_some()
}

fn extension_from_content_type(content_type: &str) -> Option<String> {
    let mime = content_type.parse::<mime::Mime>().ok()?;
    mime_guess::get_mime_extensions(&mime)
        .and_then(|exts| exts.first())
        .map(|ext| ext.to_string())
}

fn split_extension(file_name: &str) -> (String, String) {
    let path = PathBuf::from(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
        .to_string();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    (stem, ext)
}

fn sanitize_segment(segment: &str) -> String {
    let mut sanitized = String::new();
    for ch in segment.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    sanitized.trim_matches('_').to_string()
}

fn require_string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing or invalid '{key}'")))
}
