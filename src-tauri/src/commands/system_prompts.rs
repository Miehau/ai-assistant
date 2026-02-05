use crate::db::{Db, SystemPrompt, SystemPromptOperations};
use tauri::State;

#[tauri::command]
pub async fn save_system_prompt(
    state: State<'_, Db>,
    name: String,
    content: String,
) -> Result<SystemPrompt, String> {
    SystemPromptOperations::save_system_prompt(&*state, &name, &content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_system_prompt(
    state: State<'_, Db>,
    id: String,
    name: String,
    content: String,
) -> Result<SystemPrompt, String> {
    SystemPromptOperations::update_system_prompt(&*state, &id, &name, &content)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_system_prompt(
    state: State<'_, Db>,
    id: String,
) -> Result<Option<SystemPrompt>, String> {
    SystemPromptOperations::get_system_prompt(&*state, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_system_prompts(state: State<'_, Db>) -> Result<Vec<SystemPrompt>, String> {
    SystemPromptOperations::get_all_system_prompts(&*state).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_system_prompt(state: State<'_, Db>, id: String) -> Result<(), String> {
    SystemPromptOperations::delete_system_prompt(&*state, &id).map_err(|e| e.to_string())
}
