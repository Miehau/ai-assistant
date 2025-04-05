// src-tauri/src/commands/files.rs
use crate::files::{FileManager, FileMetadata, FileUploadResult};
use tauri::State;
use serde::Deserialize;
use base64::Engine;

#[derive(Debug, Deserialize)]
pub struct FileUploadPayload {
    pub file_data: String, // Base64 encoded file data
    pub file_name: String,
    pub mime_type: String,
    pub conversation_id: String,
    pub message_id: String,
}

#[tauri::command]
pub fn upload_file(
    payload: FileUploadPayload,
    file_manager: State<'_, FileManager>,
) -> Result<FileUploadResult, String> {
    // Decode the base64 data
    let file_data = match file_manager.decode_base64(&payload.file_data) {
        Ok(data) => data,
        Err(e) => {
            return Ok(FileUploadResult {
                metadata: FileMetadata {
                    id: String::new(),
                    name: payload.file_name,
                    path: String::new(),
                    mime_type: payload.mime_type,
                    size_bytes: 0,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    thumbnail_path: None,
                },
                success: false,
                error: Some(format!("Failed to decode file data: {}", e)),
            });
        }
    };

    // Save the file
    match file_manager.save_file(
        &file_data,
        &payload.file_name,
        &payload.mime_type,
        &payload.conversation_id,
        &payload.message_id,
    ) {
        Ok(metadata) => Ok(FileUploadResult {
            metadata,
            success: true,
            error: None,
        }),
        Err(e) => Ok(FileUploadResult {
            metadata: FileMetadata {
                id: String::new(),
                name: payload.file_name,
                path: String::new(),
                mime_type: payload.mime_type,
                size_bytes: 0,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                thumbnail_path: None,
            },
            success: false,
            error: Some(format!("Failed to save file: {}", e)),
        }),
    }
}

#[tauri::command]
pub fn get_file(
    file_path: String,
    as_base64: bool,
    file_manager: State<'_, FileManager>,
) -> Result<String, String> {
    match file_manager.get_file(&file_path) {
        Ok(data) => {
            if as_base64 {
                // Determine MIME type from file path
                let mime_type = mime_guess::from_path(&file_path)
                    .first_or_octet_stream()
                    .to_string();
                
                // Return as base64 with MIME type prefix
                Ok(file_manager.encode_to_base64(&data, &mime_type))
            } else {
                // Return raw binary data encoded as base64 without MIME prefix
                Ok(base64::engine::general_purpose::STANDARD.encode(data))
            }
        },
        Err(e) => Err(format!("Failed to read file: {}", e)),
    }
}

#[tauri::command]
pub fn delete_file(
    file_path: String,
    file_manager: State<'_, FileManager>,
) -> Result<bool, String> {
    match file_manager.delete_file(&file_path) {
        Ok(deleted) => Ok(deleted),
        Err(e) => Err(format!("Failed to delete file: {}", e)),
    }
}

#[tauri::command]
pub fn cleanup_empty_directories(
    file_manager: State<'_, FileManager>,
) -> Result<bool, String> {
    match file_manager.cleanup_empty_dirs() {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Failed to cleanup directories: {}", e)),
    }
}
