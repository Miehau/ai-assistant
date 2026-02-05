use crate::db::{CreateMcpServerInput, Db, McpServer, McpServerOperations, UpdateMcpServerInput};
use reqwest::blocking::Client;
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub fn get_mcp_servers(state: State<'_, Db>) -> Result<Vec<McpServer>, String> {
    McpServerOperations::get_mcp_servers(&*state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_mcp_server(state: State<'_, Db>, id: String) -> Result<Option<McpServer>, String> {
    McpServerOperations::get_mcp_server_by_id(&*state, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_mcp_server(
    state: State<'_, Db>,
    input: CreateMcpServerInput,
) -> Result<McpServer, String> {
    McpServerOperations::create_mcp_server(&*state, &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_mcp_server(
    state: State<'_, Db>,
    input: UpdateMcpServerInput,
) -> Result<Option<McpServer>, String> {
    McpServerOperations::update_mcp_server(&*state, &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_mcp_server(state: State<'_, Db>, id: String) -> Result<bool, String> {
    McpServerOperations::delete_mcp_server(&*state, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn test_mcp_server(state: State<'_, Db>, id: String) -> Result<Value, String> {
    let server = McpServerOperations::get_mcp_server_by_id(&*state, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "MCP server not found".to_string())?;

    let client = Client::new();
    let response = client
        .get(&server.url)
        .send()
        .map_err(|e| format!("Failed to reach MCP server: {e}"))?;
    let status = response.status().as_u16();
    Ok(serde_json::json!({
        "ok": status >= 200 && status < 300,
        "status": status
    }))
}
