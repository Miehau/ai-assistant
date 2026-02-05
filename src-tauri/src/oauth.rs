use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleOAuthErrorResponse {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    error_uri: Option<String>,
}

pub fn google_oauth_config() -> Result<GoogleOAuthConfig, String> {
    let client_id = google_oauth_env_value("GOOGLE_OAUTH_CLIENT_ID");
    let client_secret = google_oauth_env_value("GOOGLE_OAUTH_CLIENT_SECRET");

    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Err(
            "Google OAuth is disabled. Set GOOGLE_OAUTH_CLIENT_ID and GOOGLE_OAUTH_CLIENT_SECRET."
                .to_string(),
        );
    }

    Ok(GoogleOAuthConfig {
        client_id,
        client_secret: Some(client_secret),
    })
}

pub fn google_oauth_env_configured() -> bool {
    let client_id = google_oauth_env_value("GOOGLE_OAUTH_CLIENT_ID");
    let client_secret = google_oauth_env_value("GOOGLE_OAUTH_CLIENT_SECRET");
    !client_id.trim().is_empty() && !client_secret.trim().is_empty()
}

fn google_oauth_env_value(key: &str) -> String {
    match key {
        "GOOGLE_OAUTH_CLIENT_ID" => std::env::var(key)
            .ok()
            .or_else(|| option_env!("GOOGLE_OAUTH_CLIENT_ID").map(|value| value.to_string()))
            .unwrap_or_default(),
        "GOOGLE_OAUTH_CLIENT_SECRET" => std::env::var(key)
            .ok()
            .or_else(|| option_env!("GOOGLE_OAUTH_CLIENT_SECRET").map(|value| value.to_string()))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

pub fn generate_pkce() -> (String, String) {
    let verifier: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
    (verifier, challenge)
}

pub fn build_google_auth_url(
    config: &GoogleOAuthConfig,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    code_challenge: &str,
) -> Result<String, String> {
    let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|err| format!("Failed to build auth URL: {err}"))?;
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &scopes.join(" "))
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

pub fn exchange_google_code(
    config: &GoogleOAuthConfig,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<GoogleTokenResponse, String> {
    let client = reqwest::blocking::Client::new();
    let mut params: Vec<(&str, String)> = vec![
        ("client_id", config.client_id.clone()),
        ("code", code.to_string()),
        ("code_verifier", code_verifier.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("grant_type", "authorization_code".to_string()),
    ];
    if let Some(secret) = config.client_secret.as_ref() {
        params.push(("client_secret", secret.clone()));
    }

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .map_err(|err| format!("Token exchange failed: {err}"))?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|err| format!("Failed to read token response: {err}"))?;

    if !status.is_success() {
        let details = format_google_oauth_error(&body);
        let has_secret = config
            .client_secret
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        return Err(format!(
            "Token exchange error: HTTP {status}{details} [client_id={}, client_secret={}]",
            config.client_id,
            if has_secret { "present" } else { "absent" }
        ));
    }

    serde_json::from_str::<GoogleTokenResponse>(&body)
        .map_err(|err| format!("Failed to parse token response: {err}"))
}

pub fn refresh_google_token(
    config: &GoogleOAuthConfig,
    refresh_token: &str,
) -> Result<GoogleTokenResponse, String> {
    let client = reqwest::blocking::Client::new();
    let mut params: Vec<(&str, String)> = vec![
        ("client_id", config.client_id.clone()),
        ("refresh_token", refresh_token.to_string()),
        ("grant_type", "refresh_token".to_string()),
    ];
    if let Some(secret) = config.client_secret.as_ref() {
        params.push(("client_secret", secret.clone()));
    }

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .map_err(|err| format!("Token refresh failed: {err}"))?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|err| format!("Failed to read refresh response: {err}"))?;

    if !status.is_success() {
        let details = format_google_oauth_error(&body);
        let has_secret = config
            .client_secret
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        return Err(format!(
            "Token refresh error: HTTP {status}{details} [client_id={}, client_secret={}]",
            config.client_id,
            if has_secret { "present" } else { "absent" }
        ));
    }

    serde_json::from_str::<GoogleTokenResponse>(&body)
        .map_err(|err| format!("Failed to parse refresh response: {err}"))
}

fn format_google_oauth_error(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(parsed) = serde_json::from_str::<GoogleOAuthErrorResponse>(trimmed) {
        let mut parts: Vec<String> = Vec::new();
        if let Some(error) = parsed.error {
            if !error.trim().is_empty() {
                parts.push(error.trim().to_string());
            }
        }
        if let Some(description) = parsed.error_description {
            if !description.trim().is_empty() {
                parts.push(description.trim().to_string());
            }
        }
        if let Some(uri) = parsed.error_uri {
            if !uri.trim().is_empty() {
                parts.push(uri.trim().to_string());
            }
        }
        if !parts.is_empty() {
            return format!(" ({})", parts.join(" - "));
        }
    } else if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
            let mut parts: Vec<String> = Vec::new();
            if !error.trim().is_empty() {
                parts.push(error.trim().to_string());
            }
            if let Some(description) = value.get("error_description").and_then(|v| v.as_str()) {
                if !description.trim().is_empty() {
                    parts.push(description.trim().to_string());
                }
            }
            if let Some(uri) = value.get("error_uri").and_then(|v| v.as_str()) {
                if !uri.trim().is_empty() {
                    parts.push(uri.trim().to_string());
                }
            }
            if !parts.is_empty() {
                return format!(" ({})", parts.join(" - "));
            }
        }
    }

    let truncated: String = trimmed.chars().take(300).collect();
    if trimmed.len() > truncated.len() {
        format!(" ({}...)", truncated)
    } else {
        format!(" ({truncated})")
    }
}

#[derive(Clone, Debug)]
struct OAuthSession {
    status: OAuthStatus,
}

#[derive(Clone, Debug)]
enum OAuthStatus {
    Pending,
    Completed { connection_id: String },
    Error { message: String },
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct OAuthSessionStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct OAuthSessionStore {
    sessions: Arc<Mutex<HashMap<String, OAuthSession>>>,
}

impl OAuthSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_session(&self) -> String {
        let id = Uuid::new_v4().to_string();
        let session = OAuthSession {
            status: OAuthStatus::Pending,
        };
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(id.clone(), session);
        id
    }

    pub fn set_completed(&self, id: &str, connection_id: String) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            session.status = OAuthStatus::Completed { connection_id };
        }
    }

    pub fn set_error(&self, id: &str, message: String) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            session.status = OAuthStatus::Error { message };
        }
    }

    pub fn set_cancelled(&self, id: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            session.status = OAuthStatus::Cancelled;
        }
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        let sessions = self.sessions.lock().unwrap();
        matches!(
            sessions.get(id).map(|session| &session.status),
            Some(OAuthStatus::Cancelled)
        )
    }

    pub fn get_status(&self, id: &str) -> Option<OAuthSessionStatus> {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(id).map(|session| match &session.status {
            OAuthStatus::Pending => OAuthSessionStatus {
                status: "pending".to_string(),
                connection_id: None,
                error: None,
            },
            OAuthStatus::Completed { connection_id } => OAuthSessionStatus {
                status: "completed".to_string(),
                connection_id: Some(connection_id.clone()),
                error: None,
            },
            OAuthStatus::Error { message } => OAuthSessionStatus {
                status: "error".to_string(),
                connection_id: None,
                error: Some(message.clone()),
            },
            OAuthStatus::Cancelled => OAuthSessionStatus {
                status: "cancelled".to_string(),
                connection_id: None,
                error: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{build_google_auth_url, generate_pkce, GoogleOAuthConfig};

    #[test]
    fn pkce_generation_is_deterministic_length() {
        let (verifier, challenge) = generate_pkce();
        assert!(verifier.len() >= 43);
        assert!(!challenge.contains('='));
    }

    #[test]
    fn build_auth_url_contains_params() {
        let config = GoogleOAuthConfig {
            client_id: "client-id".to_string(),
            client_secret: None,
        };
        let scopes = vec!["scope-a".to_string(), "scope-b".to_string()];
        let url = build_google_auth_url(
            &config,
            "http://127.0.0.1:8000/callback",
            &scopes,
            "state",
            "challenge",
        )
        .expect("url");
        let parsed = url::Url::parse(&url).expect("parse url");
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(params.get("client_id"), Some(&"client-id".to_string()));
        assert_eq!(
            params.get("redirect_uri"),
            Some(&"http://127.0.0.1:8000/callback".to_string())
        );
        assert_eq!(params.get("scope"), Some(&"scope-a scope-b".to_string()));
        assert_eq!(params.get("code_challenge"), Some(&"challenge".to_string()));
    }
}
