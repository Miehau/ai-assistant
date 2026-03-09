use super::{
    build_allowlist_map, build_client, ensure_host_allowed, headers_to_json, is_text_content,
    load_allowlist, normalize_host, parse_headers, parse_method, parse_url, read_limited_body,
    require_string_arg, DEFAULT_MAX_BYTES, DEFAULT_TIMEOUT_MS, DEFAULT_USER_AGENT,
};
use crate::db::Db;
use crate::tools::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode};
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn register_request_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.request".to_string(),
        description: "Send an HTTP request and return the response (host must be approved)."
            .to_string(),
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
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let url = require_string_arg(&args, "url")?;
        let method_raw = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let method = parse_method(method_raw)?;
        let headers = parse_headers(&args)?;
        let body = args
            .get("body")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
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
