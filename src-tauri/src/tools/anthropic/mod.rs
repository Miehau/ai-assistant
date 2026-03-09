use crate::db::{Db, ModelOperations};
use crate::tools::{ToolError, ToolRegistry};
use reqwest::blocking::Client;
use serde_json::Value;
use std::time::Duration;

mod batch_create;
mod batch_poll;
mod batch_process_files;
mod batch_results;
mod file_delete;
mod file_upload;

pub(crate) const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";
pub(crate) const ANTHROPIC_BETA_FILES: &str = "files-api-2025-04-14";
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 60;
pub(crate) const RESULTS_TIMEOUT_SECS: u64 = 300;

pub fn register_anthropic_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    file_upload::register_file_upload(registry, db.clone())?;
    batch_create::register_batch_create(registry, db.clone())?;
    batch_process_files::register_batch_process_files(registry, db.clone())?;
    batch_poll::register_batch_poll(registry, db.clone())?;
    batch_results::register_batch_results(registry, db.clone())?;
    file_delete::register_file_delete(registry, db)?;
    Ok(())
}

pub(crate) fn get_anthropic_api_key(db: &Db) -> Result<String, ToolError> {
    let key = ModelOperations::get_api_key(db, "anthropic")
        .map_err(|e| ToolError::new(format!("DB error reading Anthropic API key: {e}")))?
        .ok_or_else(|| ToolError::new("Anthropic API key is not configured"))?;
    if key.trim().is_empty() {
        return Err(ToolError::new("Anthropic API key is empty"));
    }
    Ok(key)
}

pub(crate) fn anthropic_client(timeout_secs: u64) -> Result<Client, ToolError> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| ToolError::new(format!("Failed to build HTTP client: {e}")))
}

pub(crate) fn extract_anthropic_error(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
    }
    let truncated: String = body.chars().take(300).collect();
    truncated
}

pub(crate) fn validate_id(id: &str, label: &str) -> Result<String, ToolError> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(ToolError::new(format!("{label} is required")));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(ToolError::new(format!("Invalid {label}")));
    }
    Ok(id)
}

pub(crate) fn require_string(args: &Value, field: &str) -> Result<String, ToolError> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing required field: {field}")))
}
