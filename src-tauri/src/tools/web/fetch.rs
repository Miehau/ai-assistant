use super::{
    build_allowlist_map, build_client, ensure_host_allowed, extract_links_from_document,
    extract_text, extract_title, is_html_content, is_text_content, load_allowlist, normalize_host,
    parse_url, read_limited_body, require_string_arg, DEFAULT_MAX_BYTES, DEFAULT_TIMEOUT_MS,
    DEFAULT_USER_AGENT,
};
use crate::db::Db;
use crate::tools::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode};
use reqwest::header::CONTENT_TYPE;
use scraper::Html;
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn register_fetch_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.fetch".to_string(),
        description: "Fetch a web page and extract text plus links (host must be approved)."
            .to_string(),
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
                "max_links": { "type": "integer", "minimum": 1 },
                "include_headers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of response header names to include in the result (case-insensitive). Defaults to none."
                }
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
                "bytes": { "type": "integer" },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            },
            "required": ["url", "status", "content_type", "text", "links", "truncated", "bytes"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Persist,
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
        let include_headers: Vec<String> = args
            .get("include_headers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();

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
        let selected_headers: serde_json::Map<String, Value> = if !include_headers.is_empty() {
            response
                .headers()
                .iter()
                .filter(|(name, _)| include_headers.contains(&name.as_str().to_ascii_lowercase()))
                .filter_map(|(name, value)| {
                    value.to_str().ok().map(|v| (name.to_string(), json!(v)))
                })
                .collect()
        } else {
            serde_json::Map::new()
        };

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
                links =
                    extract_links_from_document(&document, &base_url, same_host_only, max_links);
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

        if !selected_headers.is_empty() {
            if let Some(obj) = result.as_object_mut() {
                obj.insert("headers".to_string(), Value::Object(selected_headers));
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
