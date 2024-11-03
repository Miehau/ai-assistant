use crate::db::{Db, ModelOperations};
use tauri::State;

#[tauri::command]
pub fn set_api_key(state: State<'_, Db>, provider: String, api_key: String) -> Result<(), String> {
    ModelOperations::set_api_key(&*state, &provider, &api_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_api_key(state: State<'_, Db>, provider: String) -> Result<Option<String>, String> {
    ModelOperations::get_api_key(&*state, &provider)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_api_key(state: State<'_, Db>, provider: String) -> Result<(), String> {
    ModelOperations::delete_api_key(&*state, &provider)
        .map_err(|e| e.to_string())
} 