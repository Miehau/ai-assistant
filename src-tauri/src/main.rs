// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;

use tauri::Manager;
use db::{Db, Conversation, Message, Model};
use std::fs;
use tauri::State;
use uuid::Uuid;

// Update the get_models function
#[tauri::command]
fn get_models(state: State<'_, Db>) -> Result<Vec<Model>, String> {
    state.get_models().map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    state.toggle_model(&model.provider, &model.model_name)
        .map_err(|e| e.to_string())
}

// Update the add_model function
#[tauri::command]
fn add_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    println!("Adding model: {:?}", model);
    state.add_model(&model).map_err(|e| e.to_string())
}

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn save_memory(state: State<'_, Db>, content: String) -> Result<String, String> {
    state.save_memory(&content)
        .map_err(|e| e.to_string())?;
    Ok("Memory saved successfully".into())
}

#[tauri::command]
fn get_or_create_conversation(state: State<'_, Db>, conversation_id: Option<String>) -> Result<Conversation, String> {
    let conversation_id = conversation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    state.inner().get_or_create_conversation(&conversation_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_message(state: State<'_, Db>, conversation_id: String, role: String, content: String) -> Result<(), String> {
    state.save_message(&conversation_id, &role, &content).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_conversation_history(state: State<'_, Db>, conversation_id: String) -> Result<Vec<Message>, String> {
    state.get_messages(&conversation_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    state.get_conversations().map_err(|e| e.to_string())
}

// Add these new commands after the existing ones
#[tauri::command]
fn set_api_key(state: State<'_, Db>, provider: String, api_key: String) -> Result<(), String> {
    state.set_api_key(&provider, &api_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_api_key(state: State<'_, Db>, provider: String) -> Result<Option<String>, String> {
    state.get_api_key(&provider)
        .map_err(|e| e.to_string())
}

// Add these new commands
#[tauri::command]
fn delete_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    state.delete_model(&model.provider, &model.model_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_api_key(state: State<'_, Db>, provider: String) -> Result<(), String> {
    state.delete_api_key(&provider)
        .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_dir = app.path_resolver().app_data_dir().expect("Failed to get app data dir");
            fs::create_dir_all(&app_dir).expect("Failed to create app directory");
            let db_path = app_dir.join("app.db");
            let mut db = Db::new(db_path.to_str().unwrap()).expect("Failed to create database");
            db.run_migrations().expect("Failed to run database migrations");
            println!("Migration status: {:?}", db.run_migrations());
            println!("Database directory: {:?}", app_dir);

            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            save_memory,
            greet,
            get_or_create_conversation,
            save_message,
            get_conversation_history,
            get_conversations,
            get_models,
            add_model,
            toggle_model,
            set_api_key,
            get_api_key,
            delete_model,
            delete_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
