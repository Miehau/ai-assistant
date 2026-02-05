use crate::db::{
    CreateCustomBackendInput, CustomBackend, CustomBackendOperations, Db, UpdateCustomBackendInput,
};
use tauri::State;

#[tauri::command]
pub fn get_custom_backends(state: State<'_, Db>) -> Result<Vec<CustomBackend>, String> {
    CustomBackendOperations::get_custom_backends(&*state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_custom_backend(
    state: State<'_, Db>,
    id: String,
) -> Result<Option<CustomBackend>, String> {
    CustomBackendOperations::get_custom_backend_by_id(&*state, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_custom_backend(
    state: State<'_, Db>,
    input: CreateCustomBackendInput,
) -> Result<CustomBackend, String> {
    CustomBackendOperations::create_custom_backend(&*state, &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_custom_backend(
    state: State<'_, Db>,
    input: UpdateCustomBackendInput,
) -> Result<Option<CustomBackend>, String> {
    CustomBackendOperations::update_custom_backend(&*state, &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_custom_backend(state: State<'_, Db>, id: String) -> Result<bool, String> {
    CustomBackendOperations::delete_custom_backend(&*state, &id).map_err(|e| e.to_string())
}
