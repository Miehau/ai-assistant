mod calendar;
mod gmail;
mod todoist;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{ToolError, ToolRegistry};
use crate::db::{
    Db, IntegrationConnection, IntegrationConnectionOperations, UpdateIntegrationConnectionInput,
};
use crate::oauth::{google_oauth_config, google_oauth_env_configured, refresh_google_token};

pub(crate) const DEFAULT_MAX_GMAIL_ATTACHMENT_BYTES: usize = 26_214_400; // 25 MiB

pub fn register_integration_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    if google_oauth_env_configured() {
        gmail::register_gmail_tools(registry, db.clone())?;
        calendar::register_google_calendar_tools(registry, db.clone())?;
    }
    todoist::register_todoist_tools(registry, db)?;
    Ok(())
}

pub(crate) fn get_connection(
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

pub(crate) fn get_access_token(connection: &IntegrationConnection) -> Result<String, ToolError> {
    let token = connection.access_token.clone().unwrap_or_default();
    if token.is_empty() {
        return Err(ToolError::new(
            "Integration connection is missing an access token",
        ));
    }
    Ok(token)
}

pub(crate) fn get_google_access_token(
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

pub(crate) fn ensure_unique_path(
    base_dir: &PathBuf,
    display_dir: &str,
    filename: &str,
) -> Result<(PathBuf, String), ToolError> {
    let target = base_dir.join(filename);
    if !target.exists() {
        return Ok((target, join_display_path(display_dir, filename)));
    }
    let (stem, ext) = split_extension(filename);
    for counter in 1..=500 {
        let candidate = if ext.is_empty() {
            format!("{stem}-{counter}")
        } else {
            format!("{stem}-{counter}.{ext}")
        };
        let candidate_path = base_dir.join(&candidate);
        if !candidate_path.exists() {
            let display_path = join_display_path(display_dir, &candidate);
            return Ok((candidate_path, display_path));
        }
    }
    Err(ToolError::new("Failed to generate unique filename"))
}

pub(crate) fn join_display_path(display_dir: &str, filename: &str) -> String {
    let trimmed = display_dir.trim();
    if trimmed.is_empty() {
        filename.to_string()
    } else {
        format!("{}/{}", trimmed.trim_end_matches('/'), filename)
    }
}

pub(crate) fn split_extension(file_name: &str) -> (String, String) {
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

pub(crate) fn sanitize_segment(segment: &str) -> String {
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

pub(crate) fn has_extension(name: &str) -> bool {
    PathBuf::from(name).extension().is_some()
}

pub(crate) fn extension_from_mime_type(mime_type: &str) -> Option<String> {
    let mime = mime_type.parse::<mime::Mime>().ok()?;
    mime_guess::get_mime_extensions(&mime)
        .and_then(|exts| exts.first())
        .map(|ext| ext.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        calendar::register_google_calendar_tools, gmail::register_gmail_tools,
        todoist::register_todoist_tools,
    };
    use crate::db::Db;
    use crate::tools::ToolRegistry;
    use serde_json::{json, Value};
    use uuid::Uuid;

    fn setup_db() -> Db {
        let db_path =
            std::env::temp_dir().join(format!("integration-tools-schema-{}.db", Uuid::new_v4()));
        let mut db = Db::new(db_path.to_str().expect("valid db path")).expect("db init failed");
        db.run_migrations().expect("db migrations failed");
        db
    }

    fn required_fields(tool_schema: &Value) -> Vec<String> {
        tool_schema
            .get("required")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    #[test]
    fn integration_tool_schemas_do_not_require_connection_id() {
        let db = setup_db();
        let mut registry = ToolRegistry::new();
        register_gmail_tools(&mut registry, db.clone()).expect("gmail tools registration failed");
        register_google_calendar_tools(&mut registry, db.clone())
            .expect("gcal tools registration failed");
        register_todoist_tools(&mut registry, db).expect("todoist tools registration failed");

        let tool_names = [
            "gmail.list_threads",
            "gmail.get_thread",
            "gmail.download_attachment",
            "gmail.list_labels",
            "gmail.draft_email",
            "gcal.list_calendars",
            "gcal.list_events",
            "gcal.create_event",
            "gcal.update_event",
            "todoist.list_tasks",
            "todoist.create_task",
            "todoist.complete_task",
        ];

        for tool_name in tool_names {
            let tool = registry.get(tool_name).expect("missing tool");
            let required = required_fields(&tool.metadata.args_schema);
            assert!(
                !required.iter().any(|field| field == "connection_id"),
                "tool {} should not require connection_id",
                tool_name
            );
        }
    }

    #[test]
    fn integration_tool_validation_allows_omitting_connection_id() {
        let db = setup_db();
        let mut registry = ToolRegistry::new();
        register_gmail_tools(&mut registry, db.clone()).expect("gmail tools registration failed");
        register_google_calendar_tools(&mut registry, db.clone())
            .expect("gcal tools registration failed");
        register_todoist_tools(&mut registry, db).expect("todoist tools registration failed");

        let cases = [
            ("gmail.list_threads", json!({})),
            ("gmail.get_thread", json!({ "thread_id": "thread-123" })),
            (
                "gmail.download_attachment",
                json!({ "message_id": "message-123", "attachment_id": "att-1" }),
            ),
            (
                "gmail.draft_email",
                json!({
                    "to": ["user@example.com"],
                    "subject": "Subject",
                    "body": "Body"
                }),
            ),
            ("gcal.list_events", json!({})),
            (
                "gcal.create_event",
                json!({
                    "summary": "Standup",
                    "start": "2026-01-01T10:00:00Z",
                    "end": "2026-01-01T10:30:00Z"
                }),
            ),
            ("gcal.update_event", json!({ "event_id": "event-123" })),
            ("todoist.list_tasks", json!({})),
            ("todoist.create_task", json!({ "content": "Ship fix" })),
            ("todoist.complete_task", json!({ "task_id": "task-123" })),
        ];

        for (tool_name, args) in cases {
            let tool = registry.get(tool_name).expect("missing tool");
            registry
                .validate_args(&tool.metadata, &args)
                .unwrap_or_else(|err| {
                    panic!(
                        "schema validation failed for {}: {}",
                        tool_name, err.message
                    )
                });
        }
    }
}
