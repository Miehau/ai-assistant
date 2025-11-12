use crate::db::{
    Db, Branch, MessageTreeNode, ConversationTree, BranchPath, BranchStats, MessageTreeConsistencyCheck,
    BranchOperations
};
use tauri::State;

#[tauri::command]
pub fn create_branch(
    state: State<'_, Db>,
    conversation_id: String,
    name: String,
) -> Result<Branch, String> {
    BranchOperations::create_branch(&*state, &conversation_id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_message_tree_node(
    state: State<'_, Db>,
    message_id: String,
    parent_message_id: Option<String>,
    branch_id: String,
    is_branch_point: bool,
) -> Result<MessageTreeNode, String> {
    BranchOperations::create_message_tree_node(
        &*state,
        &message_id,
        parent_message_id.as_deref(),
        &branch_id,
        is_branch_point,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation_branches(
    state: State<'_, Db>,
    conversation_id: String,
) -> Result<Vec<Branch>, String> {
    BranchOperations::get_conversation_branches(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation_tree(
    state: State<'_, Db>,
    conversation_id: String,
) -> Result<ConversationTree, String> {
    BranchOperations::get_conversation_tree(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_branch_path(
    state: State<'_, Db>,
    branch_id: String,
) -> Result<BranchPath, String> {
    BranchOperations::get_branch_path(&*state, &branch_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_branch(
    state: State<'_, Db>,
    branch_id: String,
    new_name: String,
) -> Result<(), String> {
    BranchOperations::rename_branch(&*state, &branch_id, &new_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_branch(
    state: State<'_, Db>,
    branch_id: String,
) -> Result<(), String> {
    BranchOperations::delete_branch(&*state, &branch_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_branch_stats(
    state: State<'_, Db>,
    conversation_id: String,
) -> Result<BranchStats, String> {
    BranchOperations::get_branch_stats(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_or_create_main_branch(
    state: State<'_, Db>,
    conversation_id: String,
) -> Result<Branch, String> {
    BranchOperations::get_or_create_main_branch(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

/// Create a new branch from a specific message in the conversation
#[tauri::command]
pub fn create_branch_from_message(
    state: State<'_, Db>,
    conversation_id: String,
    parent_message_id: String,
    branch_name: String,
) -> Result<Branch, String> {
    BranchOperations::create_branch_from_message(
        &*state,
        &conversation_id,
        &parent_message_id,
        &branch_name,
    )
    .map_err(|e| e.to_string())
}

/// Check message tree consistency
#[tauri::command]
pub fn check_message_tree_consistency(
    state: State<'_, Db>,
) -> Result<MessageTreeConsistencyCheck, String> {
    BranchOperations::check_message_tree_consistency(&*state)
        .map_err(|e| e.to_string())
}

/// Repair message tree by adding orphaned messages
#[tauri::command]
pub fn repair_message_tree(
    state: State<'_, Db>,
) -> Result<usize, String> {
    BranchOperations::repair_message_tree(&*state)
        .map_err(|e| e.to_string())
}
