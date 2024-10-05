// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;

use tauri::Manager;
use db::Db;
use std::fs;
use tauri::State;

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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_dir = app.path_resolver().app_data_dir().expect("Failed to get app data dir");
            fs::create_dir_all(&app_dir).expect("Failed to create app directory");
            let db_path = app_dir.join("app.db");
            let mut db = Db::new(db_path.to_str().unwrap()).expect("Failed to create database");
            db.run_migrations().expect("Failed to run database migrations");

            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![save_memory,greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
