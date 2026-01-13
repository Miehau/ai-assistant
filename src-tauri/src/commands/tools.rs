use crate::tools::ApprovalStore;
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub fn resolve_tool_execution_approval(
    approvals: State<'_, ApprovalStore>,
    approval_id: String,
    approved: bool,
) -> Result<(), String> {
    approvals.resolve(&approval_id, approved)
}
