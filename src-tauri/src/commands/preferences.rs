use crate::db::{Db, PreferenceOperations};
use tauri::State;

#[tauri::command]
pub async fn get_preference(state: State<'_, Db>, key: String) -> Result<Option<String>, String> {
    PreferenceOperations::get_preference(&*state, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_preference(
    state: State<'_, Db>,
    key: String,
    value: String,
) -> Result<(), String> {
    PreferenceOperations::set_preference(&*state, &key, &value).map_err(|e| e.to_string())
}
