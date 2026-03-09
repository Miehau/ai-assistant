use super::{
    build_allowlist_map, build_client, ensure_host_allowed, load_allowlist, normalize_host,
    parse_url, read_limited_body, DEFAULT_MAX_DOWNLOAD_BYTES, DEFAULT_TIMEOUT_MS,
    DEFAULT_USER_AGENT,
};
use crate::db::Db;
use crate::tools::vault::{resolve_vault_path, VaultPath};
use crate::tools::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode};
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

pub(super) fn register_download_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.download".to_string(),
        description:
            "Download one or more URLs into the vault attachments folder (host must be approved)."
                .to_string(),
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
        result_mode: ToolResultMode::Auto,
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

        let client = build_client(
            timeout_ms,
            user_agent,
            &allowed_map,
            Some(&base_host),
            same_host_only,
        )?;

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
    base_dir: &VaultPath,
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
    path: VaultPath,
) -> Result<VaultPath, ToolError> {
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
            return Ok(VaultPath {
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
