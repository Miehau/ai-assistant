#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod commands;
mod setup_default_values;
mod files;
mod events;
mod integrations;
mod oauth;
mod llm;
mod tools;
mod agent;
mod tool_outputs;

use db::Db;
use events::EventBus;
use files::FileManager;
use std::fs;
use tauri::Manager;

fn init_logging() {
    let env = env_logger::Env::default().filter_or("RUST_LOG", "info");
    let _ = env_logger::Builder::from_env(env)
        .format_timestamp_millis()
        .try_init();
}

fn main() {
    let _ = dotenvy::dotenv();
    init_logging();
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

            // Initialize event bus and forward events to the UI
            let event_bus = EventBus::new();
            let app_handle = app.handle();
            let event_rx = event_bus.subscribe();
            std::thread::spawn(move || {
                for event in event_rx {
                    let _ = app_handle.emit_all("agent_event", event);
                }
            });
            
            let mut tool_registry = tools::ToolRegistry::new();
            tools::register_file_tools(&mut tool_registry, db.clone())
                .expect("Failed to register file tools");
            tools::register_search_tool(&mut tool_registry, db.clone())
                .expect("Failed to register search tool");
            tools::register_web_tools(&mut tool_registry, db.clone())
                .expect("Failed to register web tools");
            tools::register_pref_tools(&mut tool_registry, db.clone())
                .expect("Failed to register preference tools");
            tools::register_integration_tools(&mut tool_registry, db.clone())
                .expect("Failed to register integration tools");
            tools::register_tool_output_tools(&mut tool_registry, db.clone())
                .expect("Failed to register tool output tools");
            let approval_store = tools::ApprovalStore::new();
            let oauth_store = oauth::OAuthSessionStore::new();

            app.manage(db);
            app.manage(file_manager);
            app.manage(event_bus);
            app.manage(tool_registry);
            app.manage(approval_store);
            app.manage(oauth_store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent_send_message,
            commands::agent_cancel,
            commands::agent_generate_title,
            commands::get_models,
            commands::add_model,
            commands::toggle_model,
            commands::delete_model,
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
            commands::backfill_message_usage,
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
            // Custom backends commands
            commands::get_custom_backends,
            commands::get_custom_backend,
            commands::create_custom_backend,
            commands::update_custom_backend,
            commands::delete_custom_backend,
            // Integration registry
            commands::list_integrations,
            commands::get_integration_connections,
            commands::create_integration_connection,
            commands::update_integration_connection,
            commands::delete_integration_connection,
            commands::test_integration_connection,
            commands::start_google_oauth,
            commands::get_oauth_session,
            commands::cancel_oauth_session,
            commands::list_google_calendars,
            // MCP server commands
            commands::get_mcp_servers,
            commands::get_mcp_server,
            commands::create_mcp_server,
            commands::update_mcp_server,
            commands::delete_mcp_server,
            commands::test_mcp_server,
            // User preferences commands
            commands::get_preference,
            commands::set_preference,
            // Claude CLI commands
            commands::is_claude_cli_installed,
            // Ollama commands
            commands::discover_ollama_models,
            commands::check_ollama_status,
            // Tool approval commands
            commands::resolve_tool_execution_approval,
            commands::list_tools,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
