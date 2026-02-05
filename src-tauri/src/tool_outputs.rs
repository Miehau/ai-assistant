use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::api::path;

const APP_NAMESPACE_DIR: &str = "dev.michalmlak.ai_agent";
const TOOL_OUTPUTS_DIR: &str = "tool-outputs";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolOutputRecord {
    pub id: String,
    pub tool_name: String,
    pub conversation_id: Option<String>,
    pub message_id: String,
    pub created_at: i64,
    pub success: bool,
    pub parameters: Value,
    pub output: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolOutputRef {
    pub id: String,
    pub storage: String,
    pub path: String,
    pub conversation_id: Option<String>,
    pub message_id: String,
    pub tool_name: String,
    pub created_at: i64,
    pub size_bytes: u64,
}

fn tool_outputs_root() -> Result<PathBuf, String> {
    let app_dir = path::app_data_dir(&tauri::Config::default())
        .ok_or_else(|| "Failed to get app data dir".to_string())?;
    Ok(app_dir.join(APP_NAMESPACE_DIR).join(TOOL_OUTPUTS_DIR))
}

fn tool_output_file_path(id: &str) -> Result<PathBuf, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("Tool output id is required".to_string());
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err("Invalid tool output id".to_string());
    }
    let root = tool_outputs_root()?;
    Ok(root.join(format!("{id}.json")))
}

pub fn store_tool_output(record: &ToolOutputRecord) -> Result<ToolOutputRef, String> {
    let root = tool_outputs_root()?;
    fs::create_dir_all(&root)
        .map_err(|err| format!("Failed to create tool output directory: {err}"))?;

    let file_path = tool_output_file_path(&record.id)?;
    let payload = serde_json::to_vec(record)
        .map_err(|err| format!("Failed to serialize tool output: {err}"))?;
    fs::write(&file_path, &payload).map_err(|err| format!("Failed to write tool output: {err}"))?;

    let size_bytes = fs::metadata(&file_path)
        .map_err(|err| format!("Failed to read tool output metadata: {err}"))?
        .len();

    Ok(ToolOutputRef {
        id: record.id.clone(),
        storage: "app_data".to_string(),
        path: format!("{TOOL_OUTPUTS_DIR}/{}.json", record.id),
        conversation_id: record.conversation_id.clone(),
        message_id: record.message_id.clone(),
        tool_name: record.tool_name.clone(),
        created_at: record.created_at,
        size_bytes,
    })
}

pub fn read_tool_output(id: &str) -> Result<ToolOutputRecord, String> {
    let file_path = tool_output_file_path(id)?;
    let payload = fs::read_to_string(&file_path)
        .map_err(|err| format!("Failed to read tool output: {err}"))?;
    serde_json::from_str(&payload).map_err(|err| format!("Failed to parse tool output: {err}"))
}
