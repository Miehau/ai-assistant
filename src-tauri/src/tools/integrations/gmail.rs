use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::Arc;

use super::super::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use super::{
    ensure_unique_path, extension_from_mime_type, get_connection, get_google_access_token,
    has_extension, join_display_path, sanitize_segment, DEFAULT_MAX_GMAIL_ATTACHMENT_BYTES,
};
use crate::db::Db;
use crate::tools::vault::ensure_parent_dirs;

pub(super) fn register_gmail_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let db_for_list = db.clone();
    let db_for_get = db.clone();
    let db_for_download = db.clone();
    let db_for_labels = db.clone();
    let db_for_draft = db.clone();
    let list_threads = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.list_threads".to_string(),
            description: "List Gmail threads for the connected account.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Gmail account."
                    },
                    "query": { "type": "string" },
                    "label_ids": { "type": "array", "items": { "type": "string" } },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 500 },
                    "page_token": { "type": "string" }
                }
            }),
            result_schema: json!({
                "type": "object",
                "properties": {
                    "threads": { "type": "array" },
                    "nextPageToken": { "type": "string" }
                }
            }),
            requires_approval: false,
            result_mode: ToolResultMode::Inline,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_list, connection_id, "gmail")?;
            let token = get_google_access_token(&db_for_list, &connection)?;

            let client = Client::new();
            let mut request = client.get("https://gmail.googleapis.com/gmail/v1/users/me/threads");
            if let Some(query) = args.get("query").and_then(|v| v.as_str()) {
                request = request.query(&[("q", query)]);
            }
            if let Some(label_ids) = args.get("label_ids").and_then(|v| v.as_array()) {
                for label in label_ids {
                    if let Some(label) = label.as_str() {
                        request = request.query(&[("labelIds", label)]);
                    }
                }
            }
            if let Some(max_results) = args.get("max_results").and_then(|v| v.as_u64()) {
                request = request.query(&[("maxResults", max_results.to_string())]);
            }
            if let Some(page_token) = args.get("page_token").and_then(|v| v.as_str()) {
                request = request.query(&[("pageToken", page_token)]);
            }

            let response = request
                .bearer_auth(token)
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Gmail API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Gmail API error: HTTP {status}")));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Gmail response: {err}")))
        }),
        preview: None,
    };

    let get_thread = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.get_thread".to_string(),
            description: "Get a Gmail thread with minimal fields (title, body, date, attachments)."
                .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Gmail account."
                    },
                    "thread_id": { "type": "string" },
                    "mode": { "type": "string", "enum": ["latest", "all"] },
                    "max_messages": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "required": ["thread_id"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: false,
            result_mode: ToolResultMode::Inline,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let thread_id = args.get("thread_id").and_then(|v| v.as_str()).unwrap_or("");
            if thread_id.trim().is_empty() {
                return Err(ToolError::new("Missing 'thread_id'"));
            }
            let connection = get_connection(&db_for_get, connection_id, "gmail")?;
            let token = get_google_access_token(&db_for_get, &connection)?;

            let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/threads/{thread_id}");
            let client = Client::new();
            let mut request = client.get(url);

            request = request.query(&[("format", "full")]);
            request = request.query(&[("fields", "id,messages(id,threadId,internalDate,payload(headers,body,filename,mimeType,parts(headers,body,filename,mimeType,parts)))")]);

            let response = request
                .bearer_auth(token)
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Gmail API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Gmail API error: HTTP {status}")));
            }

            let raw = response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Gmail response: {err}")))?;

            let mode = args
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("latest");
            let max_messages = args
                .get("max_messages")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            Ok(minify_gmail_thread(raw, mode, max_messages))
        }),
        preview: None,
    };

    let download_attachment = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.download_attachment".to_string(),
            description:
                "Download a Gmail attachment to the resolved path (work root by default; use path starting with 'vault' to target the vault)."
                    .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Gmail account."
                    },
                    "message_id": { "type": "string" },
                    "attachment_id": { "type": "string" },
                    "path": { "type": "string" },
                    "filename": { "type": "string" },
                    "mime_type": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1 },
                    "rename_strategy": { "type": "string", "enum": ["safe", "overwrite"] }
                },
                "required": ["message_id", "attachment_id"]
            }),
            result_schema: json!({
                "type": "object",
                "properties": {
                    "message_id": { "type": "string" },
                    "attachment_id": { "type": "string" },
                    "filename": { "type": "string" },
                    "root": { "type": "string" },
                    "path": { "type": "string" },
                    "bytes_written": { "type": "integer" },
                    "size": { "type": "integer" },
                    "mime_type": { "type": "string" }
                }
            }),
            requires_approval: false,
            result_mode: ToolResultMode::Auto,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let message_id = args
                .get("message_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if message_id.is_empty() {
                return Err(ToolError::new("Missing 'message_id'"));
            }
            let attachment_id = args
                .get("attachment_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if attachment_id.is_empty() {
                return Err(ToolError::new("Missing 'attachment_id'"));
            }

            let mut filename = args
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let mime_type = args
                .get("mime_type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let output_dir = args
                .get("output_dir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if output_dir.is_empty() {
                return Err(ToolError::new("Missing 'output_dir'"));
            }
            let display_dir = args
                .get("display_dir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let root = args
                .get("root")
                .and_then(|v| v.as_str())
                .unwrap_or("work")
                .trim()
                .to_string();
            let rename_strategy = args
                .get("rename_strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("safe");
            let max_bytes = args
                .get("max_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_MAX_GMAIL_ATTACHMENT_BYTES as u64) as usize;

            if filename.is_empty() {
                filename = format!("attachment-{attachment_id}");
            }
            if !has_extension(&filename) && !mime_type.is_empty() {
                if let Some(ext) = extension_from_mime_type(&mime_type) {
                    filename = format!("{filename}.{ext}");
                }
            }
            filename = sanitize_segment(&filename);
            if filename.is_empty() {
                filename = format!("attachment-{attachment_id}");
            }

            let connection = get_connection(&db_for_download, connection_id, "gmail")?;
            let token = get_google_access_token(&db_for_download, &connection)?;

            let url = format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{message_id}/attachments/{attachment_id}"
            );
            let client = Client::new();
            let response = client
                .get(url)
                .bearer_auth(token)
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Gmail API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Gmail API error: HTTP {status}")));
            }

            let payload = response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Gmail response: {err}")))?;
            let data = payload
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if data.is_empty() {
                return Err(ToolError::new("Gmail attachment payload missing data"));
            }
            let size = payload.get("size").and_then(|v| v.as_i64()).unwrap_or(-1);
            if size >= 0 && size as usize > max_bytes {
                return Err(ToolError::new("Attachment exceeds max_bytes"));
            }
            let bytes = URL_SAFE_NO_PAD.decode(data.as_bytes()).map_err(|err| {
                ToolError::new(format!("Failed to decode Gmail attachment: {err}"))
            })?;
            if bytes.len() > max_bytes {
                return Err(ToolError::new("Attachment exceeds max_bytes"));
            }

            let base_dir = std::path::PathBuf::from(&output_dir);
            if base_dir.exists() && !base_dir.is_dir() {
                return Err(ToolError::new("output_dir must be a directory"));
            }
            std::fs::create_dir_all(&base_dir)
                .map_err(|err| ToolError::new(format!("Failed to create output directory: {err}")))?;

            let (target_path, display_path) = if rename_strategy == "safe" {
                ensure_unique_path(&base_dir, &display_dir, &filename)?
            } else {
                let target_path = base_dir.join(&filename);
                let display_path = join_display_path(&display_dir, &filename);
                (target_path, display_path)
            };
            ensure_parent_dirs(&target_path)?;
            std::fs::write(&target_path, &bytes)
                .map_err(|err| ToolError::new(format!("Failed to write attachment: {err}")))?;

            let final_filename = target_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&filename)
                .to_string();

            let mut result = serde_json::Map::new();
            result.insert("message_id".to_string(), json!(message_id));
            result.insert("attachment_id".to_string(), json!(attachment_id));
            result.insert("filename".to_string(), json!(final_filename));
            result.insert("root".to_string(), json!(root));
            result.insert("path".to_string(), json!(display_path));
            result.insert("bytes_written".to_string(), json!(bytes.len() as i64));
            if size >= 0 {
                result.insert("size".to_string(), json!(size));
            }
            if !mime_type.is_empty() {
                result.insert("mime_type".to_string(), json!(mime_type));
            }

            Ok(Value::Object(result))
        }),
        preview: None,
    };

    let list_labels = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.list_labels".to_string(),
            description: "List Gmail labels for the connected account.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Gmail account."
                    }
                }
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: false,
            result_mode: ToolResultMode::Auto,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_labels, connection_id, "gmail")?;
            let token = get_google_access_token(&db_for_labels, &connection)?;

            let client = Client::new();
            let response = client
                .get("https://gmail.googleapis.com/gmail/v1/users/me/labels")
                .bearer_auth(token)
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Gmail API: {err}")))?;

            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Gmail API error: HTTP {status}")));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Gmail response: {err}")))
        }),
        preview: None,
    };

    let draft_email = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.draft_email".to_string(),
            description: "Save an email as a Gmail draft. All fields are optional so you can create partial drafts. Pass thread_id and in_reply_to to draft a reply inside an existing thread.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Gmail account."
                    },
                    "to": { "type": "array", "items": { "type": "string" } },
                    "cc": { "type": "array", "items": { "type": "string" } },
                    "bcc": { "type": "array", "items": { "type": "string" } },
                    "subject": { "type": "string" },
                    "body": { "type": "string" },
                    "thread_id": {
                        "type": "string",
                        "description": "Optional. Place draft inside an existing thread."
                    },
                    "in_reply_to": {
                        "type": "string",
                        "description": "Optional. Message-ID of the email being replied to (sets In-Reply-To and References headers)."
                    }
                }
            }),
            result_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "message": { "type": "object" }
                }
            }),
            requires_approval: false,
            result_mode: ToolResultMode::Inline,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_draft, connection_id, "gmail")?;
            let token = get_google_access_token(&db_for_draft, &connection)?;

            let to_list = args
                .get("to")
                .and_then(|v| v.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let cc_list = args
                .get("cc")
                .and_then(|v| v.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let bcc_list = args
                .get("bcc")
                .and_then(|v| v.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let subject = args.get("subject").and_then(|v| v.as_str()).unwrap_or("");
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let in_reply_to = args
                .get("in_reply_to")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut headers = Vec::new();
            if !to_list.is_empty() {
                headers.push(format!("To: {to_list}"));
            }
            if !cc_list.is_empty() {
                headers.push(format!("Cc: {cc_list}"));
            }
            if !bcc_list.is_empty() {
                headers.push(format!("Bcc: {bcc_list}"));
            }
            if !subject.is_empty() {
                headers.push(format!("Subject: {subject}"));
            }
            if !in_reply_to.is_empty() {
                headers.push(format!("In-Reply-To: {in_reply_to}"));
                headers.push(format!("References: {in_reply_to}"));
            }
            headers.push("MIME-Version: 1.0".to_string());
            headers.push("Content-Type: text/plain; charset=\"UTF-8\"".to_string());

            let raw_email = format!("{}\r\n\r\n{}", headers.join("\r\n"), body);
            let encoded = URL_SAFE_NO_PAD.encode(raw_email.as_bytes());

            let mut message_payload = json!({ "raw": encoded });
            if let Some(thread_id) = args.get("thread_id").and_then(|v| v.as_str()) {
                if !thread_id.trim().is_empty() {
                    message_payload["threadId"] = json!(thread_id);
                }
            }

            let client = Client::new();
            let response = client
                .post("https://gmail.googleapis.com/gmail/v1/users/me/drafts")
                .bearer_auth(token)
                .json(&json!({ "message": message_payload }))
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Gmail API: {err}")))?;

            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Gmail API error: HTTP {status}")));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Gmail response: {err}")))
        }),
        preview: None,
    };

    registry.register(list_threads)?;
    registry.register(get_thread)?;
    registry.register(download_attachment)?;
    registry.register(list_labels)?;
    registry.register(draft_email)?;
    Ok(())
}

fn minify_gmail_thread(raw: Value, mode: &str, max_messages: Option<usize>) -> Value {
    let thread_id = raw
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let messages = raw
        .get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut parsed = messages
        .into_iter()
        .filter_map(|message| parse_gmail_message(&thread_id, message))
        .collect::<Vec<_>>();

    parsed.sort_by(|a, b| a.internal_date_ms.cmp(&b.internal_date_ms));
    if mode == "latest" {
        if let Some(last) = parsed.pop() {
            return json!({
                "thread_id": thread_id,
                "mode": "latest",
                "message": last
            });
        }
        return json!({
            "thread_id": thread_id,
            "mode": "latest",
            "message": null
        });
    }

    if let Some(max) = max_messages {
        if parsed.len() > max {
            parsed = parsed.into_iter().rev().take(max).collect::<Vec<_>>();
            parsed.sort_by(|a, b| a.internal_date_ms.cmp(&b.internal_date_ms));
        }
    }

    json!({
        "thread_id": thread_id,
        "mode": "all",
        "messages": parsed
    })
}

fn parse_gmail_message(_thread_id: &str, message: Value) -> Option<GmailMessageSummary> {
    let message_id = message.get("id").and_then(|v| v.as_str())?.to_string();
    let internal_date_ms = message
        .get("internalDate")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let payload = message.get("payload").cloned().unwrap_or_else(|| json!({}));
    let headers = extract_headers(&payload);
    let subject = headers.get("subject").cloned().unwrap_or_default();
    let date_header = headers.get("date").cloned();
    let from = headers.get("from").cloned();
    let to = headers.get("to").cloned();
    let cc = headers.get("cc").cloned();

    let mut body_text: Option<String> = None;
    let mut body_html: Option<String> = None;
    let mut attachments: Vec<GmailAttachmentSummary> = Vec::new();
    collect_parts(&payload, &mut body_text, &mut body_html, &mut attachments);

    // Prefer plain text; only keep HTML when no plain text is available
    let body_html = if body_text.is_some() { None } else { body_html };

    Some(GmailMessageSummary {
        message_id,
        subject,
        date_header,
        internal_date_ms,
        from,
        to,
        cc,
        body_text,
        body_html,
        attachments,
    })
}

fn extract_headers(payload: &Value) -> std::collections::HashMap<String, String> {
    let mut headers = std::collections::HashMap::new();
    if let Some(items) = payload.get("headers").and_then(|v| v.as_array()) {
        for item in items {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let value = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if !name.is_empty() {
                headers.insert(name.to_lowercase(), value.to_string());
            }
        }
    }
    headers
}

fn collect_parts(
    payload: &Value,
    body_text: &mut Option<String>,
    body_html: &mut Option<String>,
    attachments: &mut Vec<GmailAttachmentSummary>,
) {
    let filename = payload
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mime_type = payload
        .get("mimeType")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let body = payload.get("body").cloned().unwrap_or_else(|| json!({}));
    let attachment_id = body
        .get("attachmentId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let size = body.get("size").and_then(|v| v.as_i64()).unwrap_or(0);

    if !filename.is_empty() && !attachment_id.is_empty() {
        attachments.push(GmailAttachmentSummary {
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            attachment_id: attachment_id.to_string(),
            size,
        });
    }

    if let Some(data) = body.get("data").and_then(|v| v.as_str()) {
        if mime_type == "text/plain" && body_text.is_none() {
            *body_text = decode_gmail_body(data);
        } else if mime_type == "text/html" && body_html.is_none() {
            *body_html = decode_gmail_body(data);
        }
    }

    if let Some(parts) = payload.get("parts").and_then(|v| v.as_array()) {
        for part in parts {
            collect_parts(part, body_text, body_html, attachments);
        }
    }
}

fn decode_gmail_body(data: &str) -> Option<String> {
    if data.trim().is_empty() {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(data.as_bytes()).ok()?;
    String::from_utf8(decoded).ok()
}

#[derive(Debug, Clone, serde::Serialize)]
struct GmailMessageSummary {
    message_id: String,
    subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_header: Option<String>,
    internal_date_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_html: Option<String>,
    attachments: Vec<GmailAttachmentSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct GmailAttachmentSummary {
    filename: String,
    mime_type: String,
    attachment_id: String,
    size: i64,
}
