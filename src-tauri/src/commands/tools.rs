use crate::db::Db;
use crate::tools::{
    load_tool_approval_overrides,
    set_tool_approval_override as persist_tool_approval_override,
    ApprovalStore,
    ToolMetadata,
    ToolRegistry,
};
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub fn resolve_tool_execution_approval(
    approvals: State<'_, ApprovalStore>,
    approval_id: String,
    approved: bool,
) -> Result<(), String> {
    approvals.resolve(&approval_id, approved)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_tools(
    tool_registry: State<'_, ToolRegistry>,
    db: State<'_, Db>,
) -> Result<Vec<ToolMetadata>, String> {
    let overrides = load_tool_approval_overrides(&db).unwrap_or_default();
    let mut tools = tool_registry.list_metadata();
    for tool in &mut tools {
        if let Some(value) = overrides.get(&tool.name) {
            tool.requires_approval = *value;
        }
    }
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    if tools.is_empty() {
        log::warn!("[tools] list_tools returned 0 tools");
    } else {
        log::info!("[tools] list_tools returned {} tools", tools.len());
    }
    Ok(tools)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_tool_approval_override(
    db: State<'_, Db>,
    tool_name: String,
    requires_approval: Option<bool>,
) -> Result<(), String> {
    persist_tool_approval_override(&db, &tool_name, requires_approval)
}
