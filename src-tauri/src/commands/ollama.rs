use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
const OLLAMA_TIMEOUT_SECS: u64 = 3;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub digest: String,
    pub modified_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(OLLAMA_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())
}

fn is_soft_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

fn tags_url_from_base(base_url: Option<String>) -> String {
    let base = base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let trimmed = base.trim_end_matches('/');
    let base_without_v1 = match trimmed.find("/v1") {
        Some(index) => &trimmed[..index],
        None => trimmed,
    };
    let base_clean = base_without_v1.trim_end_matches('/');
    format!("{}/api/tags", base_clean)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn discover_ollama_models(
    base_url: Option<String>,
) -> Result<Vec<OllamaModel>, String> {
    let client = build_client()?;
    let tags_url = tags_url_from_base(base_url);
    let response = client.get(tags_url).send().await;

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            if is_soft_error(&error) {
                return Ok(vec![]);
            }
            return Err(error.to_string());
        }
    };

    if !response.status().is_success() {
        return Ok(vec![]);
    }

    let payload: OllamaTagsResponse = response.json().await.map_err(|e| e.to_string())?;
    Ok(payload.models)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn check_ollama_status(base_url: Option<String>) -> Result<bool, String> {
    let client = build_client()?;
    let tags_url = tags_url_from_base(base_url);
    let response = client.get(tags_url).send().await;

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            if is_soft_error(&error) {
                return Ok(false);
            }
            return Err(error.to_string());
        }
    };

    Ok(response.status().is_success())
}
