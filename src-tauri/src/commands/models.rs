use crate::db::{Db, Model, ModelOperations};
use tauri::State;

#[tauri::command]
pub fn get_models(state: State<'_, Db>) -> Result<Vec<Model>, String> {
    ModelOperations::get_models(&*state).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    ModelOperations::toggle_model(&*state, &model.provider, &model.model_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    ModelOperations::add_model(&*state, &model).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    ModelOperations::delete_model(&*state, &model.provider, &model.model_name)
        .map_err(|e| e.to_string())
}
