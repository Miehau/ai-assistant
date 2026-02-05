// src-tauri/src/commands/files.rs
use crate::files::{FileManager, FileMetadata, FileUploadResult};
use base64::Engine;
use serde::Deserialize;
use std::fs;
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct FileUploadPayload {
    pub file_data: String, // Base64 encoded file data
    pub file_name: String,
    pub mime_type: String,
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Deserialize)]
pub struct FilePathUploadPayload {
    pub file_path: String, // Path to the file on disk
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
                    metadata: None,
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
                metadata: None,
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
        }
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
pub fn cleanup_empty_directories(file_manager: State<'_, FileManager>) -> Result<bool, String> {
    match file_manager.cleanup_empty_dirs() {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Failed to cleanup directories: {}", e)),
    }
}

// Media processing options

#[derive(Debug, Deserialize)]
pub struct ImageProcessingOptions {
    pub max_width: u32,
    pub max_height: u32,
    pub quality: u8,
}

#[tauri::command]
pub fn get_image_thumbnail(
    file_path: String,
    file_manager: State<'_, FileManager>,
) -> Result<String, String> {
    match file_manager.get_thumbnail(&file_path) {
        Ok(data) => {
            // Determine MIME type (always JPEG for thumbnails)
            let mime_type = "image/jpeg";

            // Return as base64 with MIME type prefix
            Ok(file_manager.encode_to_base64(&data, mime_type))
        }
        Err(e) => Err(format!("Failed to get thumbnail: {}", e)),
    }
}

#[tauri::command]
pub fn optimize_image(
    file_path: String,
    options: ImageProcessingOptions,
    file_manager: State<'_, FileManager>,
) -> Result<String, String> {
    match file_manager.optimize_image(
        &file_path,
        options.max_width,
        options.max_height,
        options.quality,
    ) {
        Ok(data) => {
            // Determine MIME type
            let mime_type = mime_guess::from_path(&file_path)
                .first_or_octet_stream()
                .to_string();

            // Return as base64 with MIME type prefix
            Ok(file_manager.encode_to_base64(&data, &mime_type))
        }
        Err(e) => Err(format!("Failed to optimize image: {}", e)),
    }
}

// Upload file from path instead of base64 data
#[tauri::command]
pub fn upload_file_from_path(
    payload: FilePathUploadPayload,
    file_manager: State<'_, FileManager>,
) -> Result<FileUploadResult, String> {
    // Read the file directly from the filesystem
    match fs::read(&payload.file_path) {
        Ok(file_data) => {
            // Save the file to the appropriate location
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
                        metadata: None,
                    },
                    success: false,
                    error: Some(format!("Failed to save file: {}", e)),
                }),
            }
        }
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
                metadata: None,
            },
            success: false,
            error: Some(format!("Failed to read file: {}", e)),
        }),
    }
}

// Audio processing commands

#[tauri::command]
pub fn validate_audio(
    file_data: String,
    file_manager: State<'_, FileManager>,
) -> Result<bool, String> {
    // Decode the base64 data
    let data = match file_manager.decode_base64(&file_data) {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to decode audio data: {}", e)),
    };

    // Validate the audio data
    match crate::files::AudioProcessor::validate_audio(&data) {
        Ok(valid) => Ok(valid),
        Err(e) => Err(format!("Audio validation failed: {}", e)),
    }
}

#[tauri::command]
pub fn extract_audio_metadata(
    file_path: String,
    file_manager: State<'_, FileManager>,
) -> Result<serde_json::Value, String> {
    // Get the file data
    let data = match file_manager.get_file(&file_path) {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to read audio file: {}", e)),
    };

    // Extract metadata
    match crate::files::AudioProcessor::extract_metadata(&data) {
        Ok(metadata) => Ok(metadata),
        Err(e) => Err(format!("Failed to extract audio metadata: {}", e)),
    }
}

// Text processing commands

#[tauri::command]
pub fn validate_text(
    file_data: String,
    file_manager: State<'_, FileManager>,
) -> Result<bool, String> {
    // Decode the base64 data
    let data = match file_manager.decode_base64(&file_data) {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to decode text data: {}", e)),
    };

    // Validate the text data
    match crate::files::TextProcessor::validate_text(&data) {
        Ok(valid) => Ok(valid),
        Err(e) => Err(format!("Text validation failed: {}", e)),
    }
}

#[tauri::command]
pub fn extract_text_metadata(
    file_path: String,
    file_manager: State<'_, FileManager>,
) -> Result<serde_json::Value, String> {
    // Get the file data
    let data = match file_manager.get_file(&file_path) {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to read text file: {}", e)),
    };

    // Extract metadata
    match crate::files::TextProcessor::extract_metadata(&data) {
        Ok(metadata) => Ok(metadata),
        Err(e) => Err(format!("Failed to extract text metadata: {}", e)),
    }
}

#[tauri::command]
pub fn extract_code_blocks(
    file_path: String,
    file_manager: State<'_, FileManager>,
) -> Result<Vec<(String, String)>, String> {
    // Get the file data
    let data = match file_manager.get_file(&file_path) {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to read text file: {}", e)),
    };

    // Extract code blocks
    match crate::files::TextProcessor::extract_code(&data) {
        Ok(code_blocks) => Ok(code_blocks),
        Err(e) => Err(format!("Failed to extract code blocks: {}", e)),
    }
}
