use crate::tools::ApprovalStore;
use crate::tools::ToolRegistry;
use crate::tools::ToolMetadata;
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
pub fn list_tools(tool_registry: State<'_, ToolRegistry>) -> Result<Vec<ToolMetadata>, String> {
    Ok(tool_registry.list_metadata())
}
