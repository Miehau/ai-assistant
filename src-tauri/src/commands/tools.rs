use crate::db::Db;
use crate::tools::{
    load_tool_approval_overrides, set_conversation_tool_approval_override,
    set_tool_approval_override as persist_tool_approval_override, ApprovalStore,
    PendingToolApproval, ToolMetadata, ToolRegistry,
};
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub fn resolve_tool_execution_approval(
    approvals: State<'_, ApprovalStore>,
    db: State<'_, Db>,
    approval_id: String,
    approved: bool,
    scope: Option<String>,
) -> Result<(), String> {
    if approved {
        let selected_scope = scope.unwrap_or_else(|| "once".to_string());
        if selected_scope != "once" {
            let pending = approvals
                .get_pending(&approval_id)
                .ok_or_else(|| format!("Unknown approval id: {approval_id}"))?;

            match selected_scope.as_str() {
                "conversation" => {
                    let conversation_id = pending
                        .conversation_id
                        .ok_or_else(|| "Missing conversation context for approval".to_string())?;
                    set_conversation_tool_approval_override(
                        &db,
                        &conversation_id,
                        &pending.tool_name,
                        Some(false),
                    )?;
                }
                "always" => {
                    persist_tool_approval_override(&db, &pending.tool_name, Some(false))?;
                }
                "once" => {}
                other => {
                    return Err(format!(
                        "Invalid approval scope: {other}. Expected once, conversation, or always."
                    ));
                }
            }
        }
    }

    approvals.resolve(&approval_id, approved)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_pending_tool_approvals(
    approvals: State<'_, ApprovalStore>,
) -> Result<Vec<PendingToolApproval>, String> {
    Ok(approvals.list_pending())
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
