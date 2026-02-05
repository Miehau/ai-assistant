use crate::db::{
    CreateIntegrationConnectionInput, Db, IntegrationConnection, IntegrationConnectionOperations,
    UpdateIntegrationConnectionInput,
};
use crate::integrations::{default_integrations, IntegrationMetadata};
use crate::oauth::{
    build_google_auth_url, exchange_google_code, generate_pkce, google_oauth_config,
    google_oauth_env_configured, refresh_google_token, GoogleOAuthConfig, OAuthSessionStatus,
    OAuthSessionStore,
};
use reqwest::blocking::Client;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::State;
use url::Url;

#[derive(Debug, serde::Serialize, specta::Type)]
pub struct OAuthStartResponse {
    pub session_id: String,
    pub auth_url: String,
}

#[tauri::command]
pub fn list_integrations() -> Result<Vec<IntegrationMetadata>, String> {
    let mut integrations = default_integrations();
    if !google_oauth_env_configured() {
        integrations.retain(|item| item.id != "gmail" && item.id != "google_calendar");
    }
    Ok(integrations)
}

#[tauri::command]
pub fn start_google_oauth(
    state: State<'_, Db>,
    oauth_store: State<'_, OAuthSessionStore>,
    integration_id: String,
) -> Result<OAuthStartResponse, String> {
    if !google_oauth_env_configured() {
        return Err(
            "Google OAuth is disabled. Set GOOGLE_OAUTH_CLIENT_ID and GOOGLE_OAUTH_CLIENT_SECRET."
                .to_string(),
        );
    }

    let scopes = match integration_id.as_str() {
        "gmail" => vec![
            "https://www.googleapis.com/auth/gmail.readonly".to_string(),
            "https://www.googleapis.com/auth/gmail.send".to_string(),
        ],
        "google_calendar" => vec![
            "https://www.googleapis.com/auth/calendar.readonly".to_string(),
            "https://www.googleapis.com/auth/calendar.events".to_string(),
        ],
        _ => return Err("Unsupported integration for Google OAuth.".to_string()),
    };

    let config = google_oauth_config()?;
    let (code_verifier, code_challenge) = generate_pkce();
    let state_token = uuid::Uuid::new_v4().to_string();

    let listener = TcpListener::bind("127.0.0.1:0").map_err(|err| err.to_string())?;
    let port = listener.local_addr().map_err(|err| err.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/google");
    let auth_url = build_google_auth_url(
        &config,
        &redirect_uri,
        &scopes,
        &state_token,
        &code_challenge,
    )?;

    let session_id = oauth_store.create_session();
    let session_id_for_thread = session_id.clone();
    let store = oauth_store.inner().clone();
    let db = state.inner().clone();
    let integration_id_clone = integration_id.clone();
    std::thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        let deadline = Instant::now() + Duration::from_secs(300);

        loop {
            if store.is_cancelled(&session_id_for_thread) {
                break;
            }
            if Instant::now() > deadline {
                store.set_error(&session_id_for_thread, "OAuth flow timed out.".to_string());
                break;
            }

            match listener.accept() {
                Ok((mut stream, _)) => {
                    handle_oauth_callback(
                        &mut stream,
                        &store,
                        &db,
                        &session_id_for_thread,
                        &integration_id_clone,
                        &config,
                        &redirect_uri,
                        &code_verifier,
                        &state_token,
                        &scopes,
                    );
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(err) => {
                    store.set_error(
                        &session_id_for_thread,
                        format!("OAuth listener error: {err}"),
                    );
                    break;
                }
            }
        }
    });

    Ok(OAuthStartResponse {
        session_id,
        auth_url,
    })
}

#[tauri::command]
pub fn get_oauth_session(
    oauth_store: State<'_, OAuthSessionStore>,
    session_id: String,
) -> Result<OAuthSessionStatus, String> {
    oauth_store
        .get_status(&session_id)
        .ok_or_else(|| "OAuth session not found.".to_string())
}

#[tauri::command]
pub fn cancel_oauth_session(
    oauth_store: State<'_, OAuthSessionStore>,
    session_id: String,
) -> Result<bool, String> {
    oauth_store.set_cancelled(&session_id);
    Ok(true)
}

#[tauri::command]
pub fn get_integration_connections(
    state: State<'_, Db>,
) -> Result<Vec<IntegrationConnection>, String> {
    IntegrationConnectionOperations::get_integration_connections(&*state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_integration_connection(
    state: State<'_, Db>,
    input: CreateIntegrationConnectionInput,
) -> Result<IntegrationConnection, String> {
    IntegrationConnectionOperations::create_integration_connection(&*state, &input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_integration_connection(
    state: State<'_, Db>,
    input: UpdateIntegrationConnectionInput,
) -> Result<Option<IntegrationConnection>, String> {
    IntegrationConnectionOperations::update_integration_connection(&*state, &input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_integration_connection(state: State<'_, Db>, id: String) -> Result<bool, String> {
    IntegrationConnectionOperations::delete_integration_connection(&*state, &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn test_integration_connection(state: State<'_, Db>, id: String) -> Result<Value, String> {
    let connection =
        IntegrationConnectionOperations::get_integration_connection_by_id(&*state, &id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Integration connection not found".to_string())?;

    let token = connection.access_token.clone().unwrap_or_default();
    if token.is_empty() {
        return Err("Missing access token for integration connection.".to_string());
    }

    let client = Client::new();
    let (url, method) = match connection.integration_id.as_str() {
        "gmail" => (
            "https://gmail.googleapis.com/gmail/v1/users/me/profile",
            "GET",
        ),
        "google_calendar" => (
            "https://www.googleapis.com/calendar/v3/users/me/calendarList?maxResults=1",
            "GET",
        ),
        "todoist" => ("https://api.todoist.com/rest/v2/projects", "GET"),
        _ => return Err("Unsupported integration for test.".to_string()),
    };

    let request = match method {
        "GET" => client.get(url),
        _ => client.get(url),
    };

    let response = request
        .bearer_auth(token)
        .send()
        .map_err(|e| format!("Test request failed: {e}"))?;

    let status = response.status().as_u16();
    if status >= 200 && status < 300 {
        let _ = IntegrationConnectionOperations::update_integration_connection(
            &*state,
            &UpdateIntegrationConnectionInput {
                id: connection.id.clone(),
                account_label: None,
                status: Some("connected".to_string()),
                auth_type: None,
                access_token: None,
                refresh_token: None,
                scopes: None,
                expires_at: None,
                last_error: Some(String::new()),
                last_sync_at: None,
            },
        );
        Ok(serde_json::json!({
            "ok": true,
            "status": status
        }))
    } else {
        let _ = IntegrationConnectionOperations::update_integration_connection(
            &*state,
            &UpdateIntegrationConnectionInput {
                id: connection.id.clone(),
                account_label: None,
                status: Some("error".to_string()),
                auth_type: None,
                access_token: None,
                refresh_token: None,
                scopes: None,
                expires_at: None,
                last_error: Some(format!("HTTP status {status}")),
                last_sync_at: None,
            },
        );
        Ok(serde_json::json!({
            "ok": false,
            "status": status
        }))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct GoogleCalendarListItem {
    id: String,
    summary: String,
    primary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_zone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_role: Option<String>,
}

#[tauri::command]
pub fn list_google_calendars(
    state: State<'_, Db>,
    connection_id: String,
) -> Result<Vec<GoogleCalendarListItem>, String> {
    let connection =
        IntegrationConnectionOperations::get_integration_connection_by_id(&*state, &connection_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Integration connection not found".to_string())?;

    if connection.integration_id != "google_calendar" {
        return Err("Connection is not a Google Calendar integration.".to_string());
    }

    let token = get_google_access_token(&*state, &connection)?;
    let client = Client::new();
    let response = client
        .get("https://www.googleapis.com/calendar/v3/users/me/calendarList")
        .bearer_auth(token)
        .send()
        .map_err(|e| format!("Calendar list request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Calendar list request failed: HTTP {status}"));
    }

    let json = response
        .json::<Value>()
        .map_err(|e| format!("Failed to parse calendar list: {e}"))?;
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

                    Some(GoogleCalendarListItem {
                        id,
                        summary,
                        primary,
                        time_zone,
                        access_role,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(items)
}

fn handle_oauth_callback(
    stream: &mut TcpStream,
    store: &OAuthSessionStore,
    db: &Db,
    session_id: &str,
    integration_id: &str,
    config: &GoogleOAuthConfig,
    redirect_uri: &str,
    code_verifier: &str,
    state_token: &str,
    scopes: &[String],
) {
    let mut buffer = [0u8; 4096];
    let size = match stream.read(&mut buffer) {
        Ok(size) => size,
        Err(err) => {
            store.set_error(session_id, format!("OAuth read error: {err}"));
            return;
        }
    };
    let request = String::from_utf8_lossy(&buffer[..size]);
    let request_line = request.lines().next().unwrap_or("");
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let url = format!("http://localhost{path}");

    let parsed = match Url::parse(&url) {
        Ok(parsed) => parsed,
        Err(err) => {
            store.set_error(session_id, format!("OAuth parse error: {err}"));
            return;
        }
    };

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    let mut error_description: Option<String> = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.to_string()),
            "state" => state = Some(value.to_string()),
            "error" => error = Some(value.to_string()),
            "error_description" => error_description = Some(value.to_string()),
            _ => {}
        }
    }

    if let Some(error) = error {
        let message = if let Some(description) = error_description {
            format!("OAuth error: {error} - {description}")
        } else {
            format!("OAuth error: {error}")
        };
        store.set_error(session_id, message);
        let _ = respond_html(stream, "Authorization failed. You can close this window.");
        return;
    }

    if state.as_deref() != Some(state_token) {
        store.set_error(session_id, "OAuth state mismatch.".to_string());
        let _ = respond_html(stream, "Authorization failed. You can close this window.");
        return;
    }

    let code = match code {
        Some(code) => code,
        None => {
            store.set_error(session_id, "Missing authorization code.".to_string());
            let _ = respond_html(stream, "Authorization failed. You can close this window.");
            return;
        }
    };

    let token = match exchange_google_code(config, &code, code_verifier, redirect_uri) {
        Ok(token) => token,
        Err(err) => {
            store.set_error(session_id, err);
            let _ = respond_html(stream, "Authorization failed. You can close this window.");
            return;
        }
    };

    let expires_at = token.expires_in.map(|seconds| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        now + seconds * 1000
    });

    let account_label = if integration_id == "gmail" {
        fetch_gmail_profile_email(&token.access_token)
    } else {
        None
    };

    let existing = IntegrationConnectionOperations::get_integration_connections(db)
        .ok()
        .and_then(|connections| {
            connections
                .into_iter()
                .find(|item| item.integration_id == integration_id)
        });

    let refresh_token = token.refresh_token.clone().or_else(|| {
        existing
            .as_ref()
            .and_then(|item| item.refresh_token.clone())
    });

    let scopes_value = token.scope.clone().or_else(|| Some(scopes.join(" ")));

    let connection_id = if let Some(existing) = existing {
        let update = UpdateIntegrationConnectionInput {
            id: existing.id.clone(),
            account_label,
            status: Some("connected".to_string()),
            auth_type: Some("oauth2".to_string()),
            access_token: Some(token.access_token.clone()),
            refresh_token,
            scopes: scopes_value,
            expires_at,
            last_error: Some(String::new()),
            last_sync_at: None,
        };
        match IntegrationConnectionOperations::update_integration_connection(db, &update) {
            Ok(Some(updated)) => updated.id,
            _ => existing.id,
        }
    } else {
        let input = CreateIntegrationConnectionInput {
            integration_id: integration_id.to_string(),
            account_label,
            auth_type: "oauth2".to_string(),
            access_token: Some(token.access_token.clone()),
            refresh_token,
            scopes: scopes_value,
            expires_at,
        };
        match IntegrationConnectionOperations::create_integration_connection(db, &input) {
            Ok(created) => created.id,
            Err(err) => {
                store.set_error(session_id, format!("Failed to save connection: {err}"));
                let _ = respond_html(stream, "Authorization failed. You can close this window.");
                return;
            }
        }
    };

    store.set_completed(session_id, connection_id);
    let _ = respond_html(stream, "Authorization complete. You can return to the app.");
}

fn respond_html(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn fetch_gmail_profile_email(token: &str) -> Option<String> {
    let client = Client::new();
    let response = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
        .bearer_auth(token)
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let json = response.json::<Value>().ok()?;
    json.get("emailAddress")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn get_google_access_token(db: &Db, connection: &IntegrationConnection) -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("Time error: {err}"))?
        .as_millis() as i64;

    let token = connection.access_token.clone().unwrap_or_default();
    let expires_at = connection.expires_at.unwrap_or(0);
    let has_refresh = connection
        .refresh_token
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    let needs_refresh = token.trim().is_empty() || (expires_at > 0 && expires_at <= now + 60_000);

    if needs_refresh {
        let refresh_token = connection
            .refresh_token
            .clone()
            .ok_or_else(|| "Missing refresh token for Google integration.".to_string())?;
        let config = google_oauth_config()?;
        let refreshed = refresh_google_token(&config, &refresh_token)?;
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

    if token.trim().is_empty() && !has_refresh {
        return Err("Integration connection is missing access and refresh tokens.".to_string());
    }

    Ok(token)
}
