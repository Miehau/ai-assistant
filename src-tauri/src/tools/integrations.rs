use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::{
    Db, IntegrationConnection, IntegrationConnectionOperations, UpdateIntegrationConnectionInput,
};
use crate::oauth::{google_oauth_config, refresh_google_token};
use super::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry};

pub fn register_integration_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    register_gmail_tools(registry, db.clone())?;
    register_google_calendar_tools(registry, db.clone())?;
    register_todoist_tools(registry, db)?;
    Ok(())
}

fn get_connection(db: &Db, connection_id: &str, expected_integration: &str) -> Result<IntegrationConnection, ToolError> {
    let connection = IntegrationConnectionOperations::get_integration_connection_by_id(db, connection_id)
        .map_err(|err| ToolError::new(format!("Failed to load integration connection: {err}")))?
        .ok_or_else(|| ToolError::new("Integration connection not found"))?;

    if connection.integration_id != expected_integration {
        return Err(ToolError::new(format!(
            "Connection {connection_id} is not a {expected_integration} integration"
        )));
    }
    Ok(connection)
}

fn get_access_token(connection: &IntegrationConnection) -> Result<String, ToolError> {
    let token = connection.access_token.clone().unwrap_or_default();
    if token.is_empty() {
        return Err(ToolError::new("Integration connection is missing an access token"));
    }
    Ok(token)
}

fn get_google_access_token(db: &Db, connection: &IntegrationConnection) -> Result<String, ToolError> {
    let token = get_access_token(connection)?;
    let expires_at = connection.expires_at.unwrap_or(0);
    if expires_at > 0 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| ToolError::new(format!("Time error: {err}")))?
            .as_millis() as i64;
        if expires_at <= now + 60_000 {
            let refresh_token = connection
                .refresh_token
                .clone()
                .ok_or_else(|| ToolError::new("Missing refresh token for Google integration"))?;
            let config = google_oauth_config().map_err(ToolError::new)?;
            let refreshed = refresh_google_token(&config, &refresh_token).map_err(ToolError::new)?;
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

            return Ok(new_access_token);
        }
    }
    Ok(token)
}

fn register_gmail_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let db_for_list = db.clone();
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
            requires_approval: true,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
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
            requires_approval: true,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
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
            requires_approval: true,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
            let connection = get_connection(&db_for_send, connection_id, "gmail")?;
            let token = get_google_access_token(&db_for_send, &connection)?;

            let to = args.get("to").and_then(|v| v.as_array()).ok_or_else(|| ToolError::new("Missing 'to'"))?;
            let to_list = to.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ");
            let cc_list = args.get("cc").and_then(|v| v.as_array())
                .map(|list| list.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            let bcc_list = args.get("bcc").and_then(|v| v.as_array())
                .map(|list| list.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
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
    registry.register(list_labels)?;
    registry.register(send_message)?;
    Ok(())
}

fn register_google_calendar_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let db_for_list = db.clone();
    let db_for_create = db.clone();
    let list_events = ToolDefinition {
        metadata: ToolMetadata {
            name: "gcal.list_events".to_string(),
            description: "List Google Calendar events.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "calendar_id": { "type": "string" },
                    "time_min": { "type": "string" },
                    "time_max": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 2500 }
                },
                "required": ["connection_id"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: true,
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
            let connection = get_connection(&db_for_list, connection_id, "google_calendar")?;
            let token = get_google_access_token(&db_for_list, &connection)?;

            let calendar_id = args.get("calendar_id").and_then(|v| v.as_str()).unwrap_or("primary");
            let url = format!("https://www.googleapis.com/calendar/v3/calendars/{calendar_id}/events");

            let client = Client::new();
            let mut request = client.get(url);
            if let Some(time_min) = args.get("time_min").and_then(|v| v.as_str()) {
                request = request.query(&[("timeMin", time_min)]);
            }
            if let Some(time_max) = args.get("time_max").and_then(|v| v.as_str()) {
                request = request.query(&[("timeMax", time_max)]);
            }
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

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Calendar response: {err}")))
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
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
            let connection = get_connection(&db_for_create, connection_id, "google_calendar")?;
            let token = get_google_access_token(&db_for_create, &connection)?;

            let calendar_id = args.get("calendar_id").and_then(|v| v.as_str()).unwrap_or("primary");
            let url = format!("https://www.googleapis.com/calendar/v3/calendars/{calendar_id}/events");
            let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let description = args.get("description").and_then(|v| v.as_str());
            let start = args.get("start").and_then(|v| v.as_str()).unwrap_or("");
            let end = args.get("end").and_then(|v| v.as_str()).unwrap_or("");
            let time_zone = args.get("time_zone").and_then(|v| v.as_str());
            let attendees = args.get("attendees").and_then(|v| v.as_array()).map(|items| {
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
                .map_err(|err| ToolError::new(format!("Failed to call Google Calendar API: {err}")))?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::new(format!("Google Calendar API error: HTTP {status}")));
            }

            response
                .json::<Value>()
                .map_err(|err| ToolError::new(format!("Failed to parse Calendar response: {err}")))
        }),
        preview: None,
    };

    registry.register(list_events)?;
    registry.register(create_event)?;
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
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
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
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
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
        },
        handler: std::sync::Arc::new(move |args, _ctx: ToolExecutionContext| {
            let connection_id = args.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
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
