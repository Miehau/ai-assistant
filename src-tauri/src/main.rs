// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;

use tauri::Manager;
use db::{
    Db, Conversation, Message, Model, SystemPrompt, MessageAttachment, IncomingAttachment,
    ModelOperations, MessageOperations, ConversationOperations, SystemPromptOperations
};
use std::fs;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
fn get_models(state: State<'_, Db>) -> Result<Vec<Model>, String> {
    ModelOperations::get_models(&*state).map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    ModelOperations::toggle_model(&*state, &model.provider, &model.model_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn add_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    println!("Adding model: {:?}", model);
    ModelOperations::add_model(&*state, &model).map_err(|e| e.to_string())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_or_create_conversation(state: State<'_, Db>, conversation_id: Option<String>) -> Result<Conversation, String> {
    let conversation_id = conversation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    ConversationOperations::get_or_create_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_message(
    state: State<'_, Db>, 
    conversation_id: String, 
    role: String, 
    content: String,
    attachments: Vec<IncomingAttachment>
) -> Result<(), String> {
    MessageOperations::save_message(&*state, &conversation_id, &role, &content, &attachments)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_conversation_history(state: State<'_, Db>, conversation_id: String) -> Result<Vec<Message>, String> {
    MessageOperations::get_messages(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    ConversationOperations::get_conversations(&*state)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_api_key(state: State<'_, Db>, provider: String, api_key: String) -> Result<(), String> {
    ModelOperations::set_api_key(&*state, &provider, &api_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_api_key(state: State<'_, Db>, provider: String) -> Result<Option<String>, String> {
    ModelOperations::get_api_key(&*state, &provider)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_model(state: State<'_, Db>, model: Model) -> Result<(), String> {
    ModelOperations::delete_model(&*state, &model.provider, &model.model_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_api_key(state: State<'_, Db>, provider: String) -> Result<(), String> {
    ModelOperations::delete_api_key(&*state, &provider)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_system_prompt(state: State<'_, Db>, name: String, content: String) -> Result<SystemPrompt, String> {
    SystemPromptOperations::save_system_prompt(&*state, &name, &content)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_system_prompt(state: State<'_, Db>, id: String, name: String, content: String) -> Result<SystemPrompt, String> {
    SystemPromptOperations::update_system_prompt(&*state, &id, &name, &content)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_system_prompt(state: State<'_, Db>, id: String) -> Result<Option<SystemPrompt>, String> {
    SystemPromptOperations::get_system_prompt(&*state, &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_all_system_prompts(state: State<'_, Db>) -> Result<Vec<SystemPrompt>, String> {
    SystemPromptOperations::get_all_system_prompts(&*state)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_system_prompt(state: State<'_, Db>, id: String) -> Result<(), String> {
    SystemPromptOperations::delete_system_prompt(&*state, &id)
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
            delete_api_key,
            save_system_prompt,
            update_system_prompt,
            get_system_prompt,
            get_all_system_prompts,
            delete_system_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
