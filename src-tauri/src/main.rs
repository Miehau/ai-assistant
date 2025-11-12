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
            commands::get_file,
            commands::get_image_thumbnail,
            commands::optimize_image,
            commands::delete_file,
            commands::create_file_version,
            commands::get_file_version_history,
            commands::restore_file_version,
            commands::delete_file_version,
            commands::cleanup_file_versions,
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
            commands::upload_file_from_path,
            commands::get_file,
            commands::delete_file,
            commands::cleanup_empty_directories,
            // Image processing commands
            commands::get_image_thumbnail,
            commands::optimize_image,
            // Audio processing commands
            commands::validate_audio,
            commands::extract_audio_metadata,
            // Text processing commands
            commands::validate_text,
            commands::extract_text_metadata,
            commands::extract_code_blocks,
            // Usage tracking commands
            commands::save_message_usage,
            commands::update_conversation_usage,
            commands::get_conversation_usage,
            commands::get_usage_statistics,
            commands::get_message_usage,
            // Branch management commands
            commands::create_branch,
            commands::create_message_tree_node,
            commands::get_conversation_branches,
            commands::get_conversation_tree,
            commands::get_branch_path,
            commands::rename_branch,
            commands::delete_branch,
            commands::get_branch_stats,
            commands::get_or_create_main_branch,
            commands::create_branch_from_message,
            commands::check_message_tree_consistency,
            commands::repair_message_tree,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
