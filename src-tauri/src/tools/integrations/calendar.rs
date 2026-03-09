use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::Arc;

use super::super::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use super::{get_connection, get_google_access_token};
use crate::db::{Db, PreferenceOperations};

pub(super) fn register_google_calendar_tools(
    registry: &mut ToolRegistry,
    db: Db,
) -> Result<(), String> {
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
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Google Calendar account."
                    },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 250 }
                }
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
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
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
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Google Calendar account."
                    },
                    "calendar_id": { "type": "string" },
                    "calendar_ids": { "type": "array", "items": { "type": "string" } },
                    "time_min": { "type": "string" },
                    "time_max": { "type": "string" },
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 2500 }
                }
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
        handler: Arc::new(move |args, _ctx: ToolExecutionContext| {
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
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Google Calendar account."
                    },
                    "calendar_id": { "type": "string" },
                    "summary": { "type": "string" },
                    "description": { "type": "string" },
                    "start": { "type": "string" },
                    "end": { "type": "string" },
                    "time_zone": { "type": "string" },
                    "attendees": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["summary", "start", "end"]
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
                    "connection_id": {
                        "type": "string",
                        "description": "Optional. Omit to use the default connected Google Calendar account."
                    },
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
                "required": ["event_id"]
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
