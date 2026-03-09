use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::Arc;

use super::super::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use super::{get_access_token, get_connection};
use crate::db::Db;

pub(super) fn register_todoist_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
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
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Todoist account."
                    },
                    "project_id": { "type": "string" },
                    "filter": { "type": "string" }
                }
            }),
            result_schema: json!({ "type": "array" }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
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
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Todoist account."
                    },
                    "content": { "type": "string" },
                    "description": { "type": "string" },
                    "project_id": { "type": "string" },
                    "labels": { "type": "array", "items": { "type": "string" } },
                    "priority": { "type": "integer", "minimum": 1, "maximum": 4 },
                    "due_string": { "type": "string" },
                    "due_date": { "type": "string" },
                    "due_datetime": { "type": "string" }
                },
                "required": ["content"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: true,
            result_mode: ToolResultMode::Auto,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
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
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Todoist account."
                    },
                    "task_id": { "type": "string" }
                },
                "required": ["task_id"]
            }),
            result_schema: json!({ "type": "object" }),
            requires_approval: true,
            result_mode: ToolResultMode::Inline,
        },
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
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
