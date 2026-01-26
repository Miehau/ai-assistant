use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const OLLAMA_TAGS_URL: &str = "http://localhost:11434/api/tags";
const OLLAMA_TIMEOUT_SECS: u64 = 3;

#[derive(Debug, Clone, Deserialize)]
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

#[tauri::command(rename_all = "snake_case")]
pub async fn discover_ollama_models() -> Result<Vec<OllamaModel>, String> {
    let client = build_client()?;
    let response = client.get(OLLAMA_TAGS_URL).send().await;

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
pub async fn check_ollama_status() -> Result<bool, String> {
    let client = build_client()?;
    let response = client.get(OLLAMA_TAGS_URL).send().await;

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
