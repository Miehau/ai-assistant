use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use crate::db::{
    Db, IntegrationConnection, IntegrationConnectionOperations, PreferenceOperations,
    UpdateIntegrationConnectionInput,
};
use crate::oauth::{google_oauth_config, google_oauth_env_configured, refresh_google_token};

pub fn register_integration_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    if google_oauth_env_configured() {
        register_gmail_tools(registry, db.clone())?;
        register_google_calendar_tools(registry, db.clone())?;
    }
    register_todoist_tools(registry, db)?;
    Ok(())
}

fn get_connection(
    db: &Db,
    connection_id: &str,
    expected_integration: &str,
) -> Result<IntegrationConnection, ToolError> {
    let connection_id = connection_id.trim();
    let connections = IntegrationConnectionOperations::get_integration_connections(db)
        .map_err(|err| ToolError::new(format!("Failed to load integration connections: {err}")))?;

    let pick_by_integration = |connections: &[IntegrationConnection]| {
        connections
            .iter()
            .find(|item| item.integration_id == expected_integration && item.status == "connected")
            .or_else(|| {
                connections
                    .iter()
                    .find(|item| item.integration_id == expected_integration)
            })
            .cloned()
    };

    if connection_id.is_empty() || connection_id == "default" {
        if let Some(connection) = pick_by_integration(&connections) {
            if connection.status != "connected" {
                log::warn!(
                    "[tool] using non-connected integration: id={} integration={} status={}",
                    connection.id,
                    connection.integration_id,
                    connection.status
                );
            }
            return Ok(connection);
        }

        return Err(ToolError::new("Integration connection not found"));
    }

    if let Some(connection) = connections.iter().find(|item| item.id == connection_id) {
        if connection.integration_id != expected_integration {
            return Err(ToolError::new(format!(
                "Connection {connection_id} is not a {expected_integration} integration"
            )));
        }
        return Ok(connection.clone());
    }

    let alias = connection_id.to_lowercase();
    let alias_matches_integration = alias == expected_integration
        || (alias == "gcal" && expected_integration == "google_calendar")
        || (alias == "google"
            && (expected_integration == "google_calendar" || expected_integration == "gmail"));

    if alias_matches_integration {
        if let Some(connection) = pick_by_integration(&connections) {
            log::warn!(
                "[tool] resolved integration alias '{}' to connection id={}",
                connection_id,
                connection.id
            );
            return Ok(connection);
        }
    }

    let by_label = connections.iter().find(|item| {
        item.integration_id == expected_integration
            && item
                .account_label
                .as_ref()
                .map(|label| label.eq_ignore_ascii_case(connection_id))
                .unwrap_or(false)
    });
    if let Some(connection) = by_label.cloned() {
        log::warn!(
            "[tool] resolved account label '{}' to connection id={}",
            connection_id,
            connection.id
        );
        return Ok(connection);
    }

    Err(ToolError::new("Integration connection not found"))
}

fn get_access_token(connection: &IntegrationConnection) -> Result<String, ToolError> {
    let token = connection.access_token.clone().unwrap_or_default();
    if token.is_empty() {
        return Err(ToolError::new(
            "Integration connection is missing an access token",
        ));
    }
    Ok(token)
}

fn get_google_access_token(
    db: &Db,
    connection: &IntegrationConnection,
) -> Result<String, ToolError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| ToolError::new(format!("Time error: {err}")))?
        .as_millis() as i64;

    let token = connection.access_token.clone().unwrap_or_default();
    let expires_at = connection.expires_at.unwrap_or(0);
    let has_refresh = connection
        .refresh_token
        .as_ref()
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false);

    let needs_refresh = token.trim().is_empty() || (expires_at > 0 && expires_at <= now + 60_000);

    if needs_refresh {
        log::info!(
            "[oauth] refreshing Google access token: connection_id={} integration_id={} expires_at={} now={} has_refresh={}",
            connection.id,
            connection.integration_id,
            expires_at,
            now,
            has_refresh
        );
        let refresh_token = connection
            .refresh_token
            .clone()
            .ok_or_else(|| ToolError::new("Missing refresh token for Google integration"))?;
        let config = google_oauth_config().map_err(ToolError::new)?;
        let refreshed = refresh_google_token(&config, &refresh_token).map_err(|err| {
            log::warn!(
                "[oauth] refresh failed: connection_id={} integration_id={} error={}",
                connection.id,
                connection.integration_id,
                err
            );
            ToolError::new(err)
        })?;
        let new_access_token = refreshed.access_token.clone();
        let new_refresh_token = refreshed.refresh_token.clone().unwrap_or(refresh_token);
        let new_expires_at = refreshed.expires_in.map(|seconds| now + seconds * 1000);

        let _ = IntegrationConnectionOperations::update_integration_connection(
            db,
            &UpdateIntegrationConnectionInput {
                id: connection.id.clone(),
                account_label: None,
                status: Some("connected".to_string()),
                auth_type: None,
                access_token: Some(new_access_token.clone()),
                refresh_token: Some(new_refresh_token),
                scopes: None,
                expires_at: new_expires_at,
                last_error: Some(String::new()),
                last_sync_at: None,
            },
        );

        log::info!(
            "[oauth] refresh succeeded: connection_id={} integration_id={} expires_at={}",
            connection.id,
            connection.integration_id,
            new_expires_at.unwrap_or(0)
        );
        return Ok(new_access_token);
    }

    if token.trim().is_empty() && !has_refresh {
        log::warn!(
            "[oauth] missing access and refresh tokens: connection_id={} integration_id={}",
            connection.id,
            connection.integration_id
        );
        return Err(ToolError::new(
            "Integration connection is missing access and refresh tokens",
        ));
    }

    log::debug!(
        "[oauth] using cached access token: connection_id={} integration_id={} expires_at={}",
        connection.id,
        connection.integration_id,
        expires_at
    );
    Ok(token)
}

fn register_gmail_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let db_for_list = db.clone();
    let db_for_get = db.clone();
    let db_for_labels = db.clone();
    let db_for_send = db.clone();
    let list_threads = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.list_threads".to_string(),
            description: "List Gmail threads for the connected account.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "query": { "type": "string" },
                    "label_ids": { "type": "array", "items": { "type": "string" } },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 500 },
                    "page_token": { "type": "string" }
                },
                "required": ["connection_id"]
            }),
            result_schema: json!({
                "type": "object",
                "properties": {
                    "threads": { "type": "array" },
                    "nextPageToken": { "type": "string" }
                }
            }),
            requires_approval: false,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
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
                    "connection_id": { "type": "string" },
                    "thread_id": { "type": "string" },
                    "mode": { "type": "string", "enum": ["latest", "all"] },
                    "max_messages": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "required": ["connection_id", "thread_id"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: false,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
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

    let list_labels = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.list_labels".to_string(),
            description: "List Gmail labels for the connected account.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" }
                },
                "required": ["connection_id"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: false,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
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

    let send_message = ToolDefinition {
        metadata: ToolMetadata {
            name: "gmail.send_message".to_string(),
            description: "Send a Gmail message on behalf of the connected account.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "to": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "cc": { "type": "array", "items": { "type": "string" } },
                    "bcc": { "type": "array", "items": { "type": "string" } },
                    "subject": { "type": "string" },
                    "body": { "type": "string" }
                },
                "required": ["connection_id", "to", "subject", "body"]
            }),
            result_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "threadId": { "type": "string" }
                }
            }),
            requires_approval: false,
            result_mode: ToolResultMode::Inline,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_send, connection_id, "gmail")?;
            let token = get_google_access_token(&db_for_send, &connection)?;

            let to = args
                .get("to")
                .and_then(|v| v.as_array())
                .ok_or_else(|| ToolError::new("Missing 'to'"))?;
            let to_list = to
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ");
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

            let mut headers = Vec::new();
            headers.push(format!("To: {to_list}"));
            if !cc_list.is_empty() {
                headers.push(format!("Cc: {cc_list}"));
            }
            if !bcc_list.is_empty() {
                headers.push(format!("Bcc: {bcc_list}"));
            }
            headers.push(format!("Subject: {subject}"));
            headers.push("MIME-Version: 1.0".to_string());
            headers.push("Content-Type: text/plain; charset=\"UTF-8\"".to_string());

            let raw_email = format!("{}\r\n\r\n{}", headers.join("\r\n"), body);
            let encoded = URL_SAFE_NO_PAD.encode(raw_email.as_bytes());

            let client = Client::new();
            let response = client
                .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
                .bearer_auth(token)
                .json(&json!({ "raw": encoded }))
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
    registry.register(list_labels)?;
    registry.register(send_message)?;
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

fn parse_gmail_message(thread_id: &str, message: Value) -> Option<GmailMessageSummary> {
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

    let mut body_text: Option<String> = None;
    let mut body_html: Option<String> = None;
    let mut attachments: Vec<GmailAttachmentSummary> = Vec::new();
    collect_parts(&payload, &mut body_text, &mut body_html, &mut attachments);

    Some(GmailMessageSummary {
        thread_id: thread_id.to_string(),
        message_id,
        title: subject,
        date_header,
        internal_date_ms,
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
    thread_id: String,
    message_id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_header: Option<String>,
    internal_date_ms: i64,
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

fn register_google_calendar_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let db_for_list = db.clone();
    let db_for_list_calendars = db.clone();
    let db_for_create = db.clone();
    let db_for_update = db.clone();
    let list_calendars = ToolDefinition {
        metadata: ToolMetadata {
            name: "gcal.list_calendars".to_string(),
            description: "List Google Calendar calendars for the connected account. By default, returns only the user's selected calendars from integration settings (falls back to primary if none).".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 250 }
                },
                "required": ["connection_id"]
            }),
            result_schema: json!({
                "type": "object",
                "properties": {
                    "calendars": { "type": "array" }
                }
            }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
            let connection = get_connection(&db_for_list_calendars, connection_id, "google_calendar")?;
            let token = get_google_access_token(&db_for_list_calendars, &connection)?;

            let client = Client::new();
            let mut request = client.get("https://www.googleapis.com/calendar/v3/users/me/calendarList");
            if let Some(max_results) = args.get("max_results").and_then(|v| v.as_u64()) {
                request = request.query(&[("maxResults", max_results.to_string())]);
            }

            let response = request
                .bearer_auth(token)
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Google Calendar API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Google Calendar API error: HTTP {status}")));
            }

            let json = response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Calendar response: {err}")))?;

            let items = json
                .get("items")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|item| {
                            let id = item.get("id")?.as_str()?.to_string();
                            let summary = item
                                .get("summary")
                                .and_then(|value| value.as_str())
                                .unwrap_or(&id)
                                .to_string();
                            let primary = item
                                .get("primary")
                                .and_then(|value| value.as_bool())
                                .unwrap_or(false);
                            let time_zone = item
                                .get("timeZone")
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string());
                            let access_role = item
                                .get("accessRole")
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string());

                            Some(json!({
                                "id": id,
                                "summary": summary,
                                "primary": primary,
                                "time_zone": time_zone,
                                "access_role": access_role
                            }))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let pref_key = format!("integration_settings.google_calendar.{}", connection.id);
            let preferred_ids = PreferenceOperations::get_preference(&db_for_list_calendars, &pref_key)
                .ok()
                .flatten()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .and_then(|value| value.get("calendar_ids").and_then(|v| v.as_array()).cloned())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                })
                .filter(|values| !values.is_empty());

            let filtered = if let Some(ids) = preferred_ids {
                let allowed: std::collections::HashSet<_> = ids.into_iter().collect();
                items
                    .into_iter()
                    .filter(|item| {
                        item.get("id")
                            .and_then(|value| value.as_str())
                            .map(|id| allowed.contains(id))
                            .unwrap_or(false)
                    })
                    .collect::<Vec<_>>()
            } else {
                let primary = items
                    .iter()
                    .find(|item| item.get("primary").and_then(|value| value.as_bool()) == Some(true))
                    .cloned()
                    .into_iter()
                    .collect::<Vec<_>>();
                primary
            };

            Ok(json!({ "calendars": filtered }))
        }),
        preview: None,
    };

    let list_events = ToolDefinition {
        metadata: ToolMetadata {
            name: "gcal.list_events".to_string(),
            description: "List or search Google Calendar events, grouped by calendar. If calendar_ids is omitted, uses the user's selected calendars (integration settings); falls back to primary if none."
                .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "calendar_id": { "type": "string" },
                    "calendar_ids": { "type": "array", "items": { "type": "string" } },
                    "time_min": { "type": "string" },
                    "time_max": { "type": "string" },
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 2500 }
                },
                "required": ["connection_id"]
            }),
            result_schema: json!({
                "type": "object",
                "properties": {
                    "calendars": { "type": "array" }
                }
            }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
            let connection = get_connection(&db_for_list, connection_id, "google_calendar")?;
            let token = get_google_access_token(&db_for_list, &connection)?;

            let client = Client::new();
            let explicit_calendar_ids = args
                .get("calendar_ids")
                .and_then(|v| v.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                })
                .filter(|values| !values.is_empty());

            let explicit_calendar_id = args
                .get("calendar_id")
                .and_then(|v| v.as_str())
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string());

            let calendar_ids = if let Some(ids) = explicit_calendar_ids {
                ids
            } else if let Some(id) = explicit_calendar_id {
                vec![id]
            } else {
                let pref_key = format!("integration_settings.google_calendar.{}", connection.id);
                let preferred_ids = PreferenceOperations::get_preference(&db_for_list, &pref_key)
                    .ok()
                    .flatten()
                    .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                    .and_then(|value| value.get("calendar_ids").and_then(|v| v.as_array()).cloned())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(|value| value.trim())
                            .filter(|value| !value.is_empty())
                            .map(|value| value.to_string())
                            .collect::<Vec<_>>()
                    })
                    .filter(|values| !values.is_empty());

                preferred_ids.unwrap_or_else(|| vec!["primary".to_string()])
            };

            let mut grouped: Vec<Value> = Vec::new();
            for calendar_id in calendar_ids {
                let url = format!("https://www.googleapis.com/calendar/v3/calendars/{calendar_id}/events");
                let mut request = client.get(url);
                if let Some(time_min) = args.get("time_min").and_then(|v| v.as_str()) {
                    request = request.query(&[("timeMin", time_min)]);
                }
                if let Some(time_max) = args.get("time_max").and_then(|v| v.as_str()) {
                    request = request.query(&[("timeMax", time_max)]);
                }
                if let Some(query) = args.get("query").and_then(|v| v.as_str()) {
                    request = request.query(&[("q", query)]);
                }
                if let Some(max_results) = args.get("max_results").and_then(|v| v.as_u64()) {
                    request = request.query(&[("maxResults", max_results.to_string())]);
                }

                let response = request
                    .bearer_auth(&token)
                    .send()
                    .map_err(|err| ToolError::new(format!("Failed to call Google Calendar API: {err}")))?;
                let status = response.status();
                if !status.is_success() {
                    return Err(ToolError::new(format!("Google Calendar API error: HTTP {status}")));
                }

                let json = response
                    .json::<Value>()
                    .map_err(|err| ToolError::new(format!("Failed to parse Calendar response: {err}")))?;

                let events = json.get("items").cloned().unwrap_or_else(|| json!([]));
                let mut entry = serde_json::Map::new();
                entry.insert("calendar_id".to_string(), json!(calendar_id));
                entry.insert("events".to_string(), events);
                if let Some(token) = json.get("nextPageToken").and_then(|value| value.as_str()) {
                    entry.insert("next_page_token".to_string(), json!(token));
                }
                if let Some(summary) = json.get("summary").and_then(|value| value.as_str()) {
                    entry.insert("summary".to_string(), json!(summary));
                }
                if let Some(time_zone) = json.get("timeZone").and_then(|value| value.as_str()) {
                    entry.insert("time_zone".to_string(), json!(time_zone));
                }
                grouped.push(Value::Object(entry));
            }

            Ok(json!({ "calendars": grouped }))
        }),
        preview: None,
    };

    let create_event = ToolDefinition {
        metadata: ToolMetadata {
            name: "gcal.create_event".to_string(),
            description: "Create a Google Calendar event.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "calendar_id": { "type": "string" },
                    "summary": { "type": "string" },
                    "description": { "type": "string" },
                    "start": { "type": "string" },
                    "end": { "type": "string" },
                    "time_zone": { "type": "string" },
                    "attendees": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["connection_id", "summary", "start", "end"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_create, connection_id, "google_calendar")?;
            let token = get_google_access_token(&db_for_create, &connection)?;

            let calendar_id = args
                .get("calendar_id")
                .and_then(|v| v.as_str())
                .unwrap_or("primary");
            let url =
                format!("https://www.googleapis.com/calendar/v3/calendars/{calendar_id}/events");
            let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let description = args.get("description").and_then(|v| v.as_str());
            let start = args.get("start").and_then(|v| v.as_str()).unwrap_or("");
            let end = args.get("end").and_then(|v| v.as_str()).unwrap_or("");
            let time_zone = args.get("time_zone").and_then(|v| v.as_str());
            let attendees = args
                .get("attendees")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(|email| json!({ "email": email }))
                        .collect::<Vec<_>>()
                });

            let event = json!({
                "summary": summary,
                "description": description,
                "start": {
                    "dateTime": start,
                    "timeZone": time_zone
                },
                "end": {
                    "dateTime": end,
                    "timeZone": time_zone
                },
                "attendees": attendees
            });

            let client = Client::new();
            let response = client
                .post(url)
                .bearer_auth(token)
                .json(&event)
                .send()
                .map_err(|err| {
                    ToolError::new(format!("Failed to call Google Calendar API: {err}"))
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!(
                    "Google Calendar API error: HTTP {status}"
                )));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Calendar response: {err}")))
        }),
        preview: None,
    };

    let update_event = ToolDefinition {
        metadata: ToolMetadata {
            name: "gcal.update_event".to_string(),
            description: "Update fields on an existing Google Calendar event.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "event_id": { "type": "string" },
                    "calendar_id": { "type": "string" },
                    "summary": { "type": "string" },
                    "description": { "type": "string" },
                    "location": { "type": "string" },
                    "start": { "type": "string" },
                    "end": { "type": "string" },
                    "time_zone": { "type": "string" },
                    "attendees": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["connection_id", "event_id"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_update, connection_id, "google_calendar")?;
            let token = get_google_access_token(&db_for_update, &connection)?;

            let event_id = args
                .get("event_id")
                .and_then(|v| v.as_str())
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ToolError::new("event_id is required"))?;
            let calendar_id = args
                .get("calendar_id")
                .and_then(|v| v.as_str())
                .unwrap_or("primary");
            let url = format!(
                "https://www.googleapis.com/calendar/v3/calendars/{calendar_id}/events/{event_id}"
            );

            let mut event = serde_json::Map::new();

            if let Some(summary) = args.get("summary").and_then(|v| v.as_str()) {
                event.insert("summary".to_string(), json!(summary));
            }
            if let Some(description) = args.get("description").and_then(|v| v.as_str()) {
                event.insert("description".to_string(), json!(description));
            }
            if let Some(location) = args.get("location").and_then(|v| v.as_str()) {
                event.insert("location".to_string(), json!(location));
            }

            let time_zone = args.get("time_zone").and_then(|v| v.as_str());
            if let Some(start) = args.get("start").and_then(|v| v.as_str()) {
                event.insert(
                    "start".to_string(),
                    json!({
                        "dateTime": start,
                        "timeZone": time_zone
                    }),
                );
            }
            if let Some(end) = args.get("end").and_then(|v| v.as_str()) {
                event.insert(
                    "end".to_string(),
                    json!({
                        "dateTime": end,
                        "timeZone": time_zone
                    }),
                );
            }

            if let Some(attendees) = args.get("attendees").and_then(|v| v.as_array()) {
                let values = attendees
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|email| json!({ "email": email }))
                    .collect::<Vec<_>>();
                event.insert("attendees".to_string(), json!(values));
            }

            if event.is_empty() {
                return Err(ToolError::new(
                    "Provide at least one field to update (for example summary, start, or end)",
                ));
            }

            let client = Client::new();
            let response = client
                .patch(url)
                .bearer_auth(token)
                .json(&Value::Object(event))
                .send()
                .map_err(|err| {
                    ToolError::new(format!("Failed to call Google Calendar API: {err}"))
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!(
                    "Google Calendar API error: HTTP {status}"
                )));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Calendar response: {err}")))
        }),
        preview: None,
    };

    registry.register(list_calendars)?;
    registry.register(list_events)?;
    registry.register(create_event)?;
    registry.register(update_event)?;
    Ok(())
}

fn register_todoist_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let db_for_list = db.clone();
    let db_for_create = db.clone();
    let db_for_complete = db.clone();
    let list_tasks = ToolDefinition {
        metadata: ToolMetadata {
            name: "todoist.list_tasks".to_string(),
            description: "List Todoist tasks.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "project_id": { "type": "string" },
                    "filter": { "type": "string" }
                },
                "required": ["connection_id"]
            }),
            result_schema: json!({ "type": "array" }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_list, connection_id, "todoist")?;
            let token = get_access_token(&connection)?;

            let client = Client::new();
            let mut request = client.get("https://api.todoist.com/rest/v2/tasks");
            if let Some(project_id) = args.get("project_id").and_then(|v| v.as_str()) {
                request = request.query(&[("project_id", project_id)]);
            }
            if let Some(filter) = args.get("filter").and_then(|v| v.as_str()) {
                request = request.query(&[("filter", filter)]);
            }

            let response = request
                .bearer_auth(token)
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Todoist API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Todoist API error: HTTP {status}")));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Todoist response: {err}")))
        }),
        preview: None,
    };

    let create_task = ToolDefinition {
        metadata: ToolMetadata {
            name: "todoist.create_task".to_string(),
            description: "Create a Todoist task.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "content": { "type": "string" },
                    "description": { "type": "string" },
                    "project_id": { "type": "string" },
                    "labels": { "type": "array", "items": { "type": "string" } },
                    "priority": { "type": "integer", "minimum": 1, "maximum": 4 },
                    "due_string": { "type": "string" },
                    "due_date": { "type": "string" },
                    "due_datetime": { "type": "string" }
                },
                "required": ["connection_id", "content"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_create, connection_id, "todoist")?;
            let token = get_access_token(&connection)?;

            let mut payload = serde_json::Map::new();
            if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
                payload.insert("content".to_string(), json!(content));
            }
            if let Some(description) = args.get("description").and_then(|v| v.as_str()) {
                payload.insert("description".to_string(), json!(description));
            }
            if let Some(project_id) = args.get("project_id").and_then(|v| v.as_str()) {
                payload.insert("project_id".to_string(), json!(project_id));
            }
            if let Some(labels) = args.get("labels").and_then(|v| v.as_array()) {
                let labels = labels.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>();
                payload.insert("labels".to_string(), json!(labels));
            }
            if let Some(priority) = args.get("priority").and_then(|v| v.as_i64()) {
                payload.insert("priority".to_string(), json!(priority));
            }
            if let Some(due_string) = args.get("due_string").and_then(|v| v.as_str()) {
                payload.insert("due_string".to_string(), json!(due_string));
            }
            if let Some(due_date) = args.get("due_date").and_then(|v| v.as_str()) {
                payload.insert("due_date".to_string(), json!(due_date));
            }
            if let Some(due_datetime) = args.get("due_datetime").and_then(|v| v.as_str()) {
                payload.insert("due_datetime".to_string(), json!(due_datetime));
            }

            let client = Client::new();
            let response = client
                .post("https://api.todoist.com/rest/v2/tasks")
                .bearer_auth(token)
                .json(&Value::Object(payload))
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Todoist API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Todoist API error: HTTP {status}")));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Todoist response: {err}")))
        }),
        preview: None,
    };

    let complete_task = ToolDefinition {
        metadata: ToolMetadata {
            name: "todoist.complete_task".to_string(),
            description: "Mark a Todoist task as complete.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "task_id": { "type": "string" }
                },
                "required": ["connection_id", "task_id"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: true,
            result_mode: ToolResultMode::Inline,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let connection = get_connection(&db_for_complete, connection_id, "todoist")?;
            let token = get_access_token(&connection)?;

            let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            if task_id.is_empty() {
                return Err(ToolError::new("Missing task_id"));
            }

            let client = Client::new();
            let url = format!("https://api.todoist.com/rest/v2/tasks/{task_id}/close");
            let response = client
                .post(url)
                .bearer_auth(token)
                .send()
                .map_err(|err| ToolError::new(format!("Failed to call Todoist API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Todoist API error: HTTP {status}")));
            }

            Ok(json!({ "ok": true }))
        }),
        preview: None,
    };

    registry.register(list_tasks)?;
    registry.register(create_task)?;
    registry.register(complete_task)?;
    Ok(())
}
