#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod commands;
mod setup_default_values;
mod files;

use db::Db;
use files::FileManager;
use std::fs;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_dir = app.path_resolver().app_data_dir().expect("Failed to get app data dir");
            fs::create_dir_all(&app_dir).expect("Failed to create app directory");
            let db_path = app_dir.join("app.db");
            let mut db = Db::new(db_path.to_str().unwrap()).expect("Failed to create database");
            db.run_migrations().expect("Failed to run database migrations");
            
            setup_default_values::initialize(&mut db).expect("Failed to initialize default values");
            
            // Initialize the file manager
            let file_manager = FileManager::new().expect("Failed to create file manager");
            
            app.manage(db);
            app.manage(file_manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_models,
            commands::add_model,
            commands::toggle_model,
            commands::delete_model,
            commands::set_api_key,
            commands::get_api_key,
            commands::delete_api_key,
            commands::get_or_create_conversation,
            commands::save_message,
            commands::get_conversation_history,
            commands::get_conversations,
            commands::update_conversation_name,
            commands::delete_conversation,
            commands::save_system_prompt,
            commands::update_system_prompt,
            commands::get_system_prompt,
            commands::get_all_system_prompts,
            commands::delete_system_prompt,
            // File management commands
            commands::upload_file,
            commands::get_file,
            commands::delete_file,
            commands::cleanup_empty_directories,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
