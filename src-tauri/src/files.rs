// src-tauri/src/files.rs
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::api::path;
use uuid::Uuid;
use base64::Engine;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileMetadata {
    pub id: String,
    pub name: String,
    pub path: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileUploadResult {
    pub metadata: FileMetadata,
    pub success: bool,
    pub error: Option<String>,
}

pub struct FileManager {
    root_dir: PathBuf,
}

impl FileManager {
    pub fn new() -> Result<Self, io::Error> {
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Failed to get app data dir"))?;
        
        let root_dir = app_dir.join("dev.michalmlak.ai_agent").join("attachments");
        fs::create_dir_all(&root_dir)?;
        
        Ok(Self { root_dir })
    }
    
    // Create the hierarchical directory structure for a specific conversation and message
    pub fn ensure_message_directory(&self, conversation_id: &str, message_id: &str) -> Result<PathBuf, io::Error> {
        let dir_path = self.root_dir
            .join(conversation_id)
            .join(message_id);
            
        fs::create_dir_all(&dir_path)?;
        Ok(dir_path)
    }
    
    // Save a file to the appropriate directory
    pub fn save_file(
        &self, 
        data: &[u8], 
        file_name: &str, 
        mime_type: &str, 
        conversation_id: &str, 
        message_id: &str
    ) -> Result<FileMetadata, io::Error> {
        let dir_path = self.ensure_message_directory(conversation_id, message_id)?;
        
        // Generate a unique ID for the file
        let file_id = Uuid::new_v4().to_string();
        
        // Get file extension from original name or mime type
        let extension = Path::new(file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_else(|| {
                mime_type.split('/').nth(1).unwrap_or("bin")
            });
            
        // Create a filename with the unique ID and original extension
        let unique_filename = format!("{}.{}", file_id, extension);
        let file_path = dir_path.join(&unique_filename);
        
        // Write the file to disk
        fs::write(&file_path, data)?;
        
        // Get file metadata
        let metadata = fs::metadata(&file_path)?;
        let now = Utc::now();
        
        // Create relative path from root_dir
        let relative_path = format!("{}/{}/{}", conversation_id, message_id, unique_filename);
        
        Ok(FileMetadata {
            id: file_id,
            name: file_name.to_string(),
            path: relative_path,
            mime_type: mime_type.to_string(),
            size_bytes: metadata.len(),
            created_at: now,
            updated_at: now,
            thumbnail_path: None,
        })
    }
    
    // Get a file by its path
    pub fn get_file(&self, file_path: &str) -> Result<Vec<u8>, io::Error> {
        let full_path = self.root_dir.join(file_path);
        fs::read(full_path)
    }
    
    // Delete a file by its path
    pub fn delete_file(&self, file_path: &str) -> Result<bool, io::Error> {
        let full_path = self.root_dir.join(file_path);
        if full_path.exists() {
            fs::remove_file(full_path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    // Convert base64 data to binary
    pub fn decode_base64(&self, data: &str) -> Result<Vec<u8>, io::Error> {
        let base64_data = if data.contains(",") {
            data.split(",").nth(1).unwrap_or(data)
        } else {
            data
        };
        
        base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    
    // Convert binary data to base64 with MIME type prefix
    pub fn encode_to_base64(&self, data: &[u8], mime_type: &str) -> String {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(data);
        format!("data:{};base64,{}", mime_type, base64_data)
    }
    
    // Get the full path to a file
    pub fn get_full_path(&self, relative_path: &str) -> PathBuf {
        self.root_dir.join(relative_path)
    }
    
    // Clean up empty directories
    pub fn cleanup_empty_dirs(&self) -> Result<(), io::Error> {
        self.cleanup_dir(&self.root_dir)
    }
    
    fn cleanup_dir(&self, dir: &Path) -> Result<(), io::Error> {
        if dir.is_dir() {
            let entries = fs::read_dir(dir)?;
            let mut is_empty = true;
            
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_dir() {
                    self.cleanup_dir(&path)?;
                    // Check if directory is now empty after potential cleanup
                    if fs::read_dir(&path)?.next().is_none() {
                        fs::remove_dir(path)?;
                    } else {
                        is_empty = false;
                    }
                } else {
                    is_empty = false;
                }
            }
            
            // If this is not the root directory and it's empty, remove it
            if is_empty && dir != &self.root_dir {
                fs::remove_dir(dir)?;
            }
        }
        
        Ok(())
    }
}
