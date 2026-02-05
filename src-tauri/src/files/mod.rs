// src-tauri/src/files/mod.rs
mod audio;
mod image;
mod text;

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::api::path;
use uuid::Uuid;

mod versioning;

pub use audio::AudioProcessor;
pub use image::ImageProcessor;
pub use text::TextProcessor;
pub use versioning::{VersionHistory, VersionManager, VersionMetadata};

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
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileUploadResult {
    pub metadata: FileMetadata,
    pub success: bool,
    pub error: Option<String>,
}

pub struct FileManager {
    root_dir: PathBuf,
    version_manager: Option<VersionManager>,
}

impl FileManager {
    pub fn new() -> Result<Self, io::Error> {
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Failed to get app data dir"))?;

        let root_dir = app_dir.join("dev.michalmlak.ai_agent").join("attachments");
        fs::create_dir_all(&root_dir)?;

        // Initialize version manager
        let version_manager = match VersionManager::new(&root_dir) {
            Ok(vm) => Some(vm),
            Err(e) => {
                log::warn!(
                    "Failed to initialize version manager; versioning disabled: {}",
                    e
                );
                None
            }
        };

        Ok(Self {
            root_dir,
            version_manager,
        })
    }

    // Create the hierarchical directory structure for a specific conversation and message
    pub fn ensure_message_directory(
        &self,
        conversation_id: &str,
        message_id: &str,
    ) -> Result<PathBuf, io::Error> {
        let dir_path = self.root_dir.join(conversation_id).join(message_id);

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
        message_id: &str,
    ) -> Result<FileMetadata, io::Error> {
        let dir_path = self.ensure_message_directory(conversation_id, message_id)?;

        // Generate a unique ID for the file
        let file_id = Uuid::new_v4().to_string();

        // Get file extension from original name or mime type
        let extension = Path::new(file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_else(|| mime_type.split('/').nth(1).unwrap_or("bin"));

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

        // Generate thumbnail for images
        let thumbnail_path = if mime_type.starts_with("image/") {
            self.generate_image_thumbnail(data, &dir_path, &file_id)
                .ok()
                .map(|thumb_filename| {
                    format!("{}/{}/{}", conversation_id, message_id, thumb_filename)
                })
        } else {
            None
        };

        // Extract additional metadata based on file type
        let additional_metadata = if mime_type.starts_with("image/") {
            ImageProcessor::extract_metadata(data).ok()
        } else if mime_type.starts_with("audio/") {
            AudioProcessor::extract_metadata(data).ok()
        } else if mime_type.starts_with("text/")
            || mime_type.contains("json")
            || mime_type.contains("xml")
        {
            TextProcessor::extract_metadata(data).ok()
        } else {
            None
        };

        Ok(FileMetadata {
            id: file_id,
            name: file_name.to_string(),
            path: relative_path,
            mime_type: mime_type.to_string(),
            size_bytes: metadata.len(),
            created_at: now,
            updated_at: now,
            thumbnail_path,
            metadata: additional_metadata,
        })
    }

    // Generate a thumbnail for an image
    fn generate_image_thumbnail(
        &self,
        data: &[u8],
        dir_path: &Path,
        file_id: &str,
    ) -> Result<String, io::Error> {
        // Generate thumbnail using ImageProcessor
        let thumbnail_data = ImageProcessor::generate_thumbnail(data, 200, 200)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Save thumbnail
        let thumbnail_filename = format!("{}.thumbnail.jpg", file_id);
        let thumbnail_path = dir_path.join(&thumbnail_filename);
        fs::write(&thumbnail_path, thumbnail_data)?;

        Ok(thumbnail_filename)
    }

    // Get a file by its path
    pub fn get_file(&self, file_path: &str) -> Result<Vec<u8>, io::Error> {
        let full_path = self.root_dir.join(file_path);
        fs::read(full_path)
    }

    // Get a thumbnail for an image file
    pub fn get_thumbnail(&self, file_path: &str) -> Result<Vec<u8>, io::Error> {
        // Extract the directory and filename parts
        let path = Path::new(file_path);
        let parent = path.parent().unwrap_or(Path::new(""));

        // Get the file stem (filename without extension)
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        // Construct the thumbnail path
        let thumbnail_path = parent.join(format!("{}.thumbnail.jpg", file_stem));
        let full_path = self.root_dir.join(thumbnail_path);

        // If thumbnail exists, return it
        if full_path.exists() {
            fs::read(full_path)
        } else {
            // If no thumbnail exists, generate one on-the-fly
            let original_data = self.get_file(file_path)?;
            let thumbnail_data = ImageProcessor::generate_thumbnail(&original_data, 200, 200)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            Ok(thumbnail_data)
        }
    }

    // Optimize an image file
    pub fn optimize_image(
        &self,
        file_path: &str,
        max_width: u32,
        max_height: u32,
        quality: u8,
    ) -> Result<Vec<u8>, io::Error> {
        let original_data = self.get_file(file_path)?;

        ImageProcessor::optimize_image(&original_data, max_width, max_height, quality)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    // Delete a file by its path
    pub fn delete_file(&self, file_path: &str) -> Result<bool, io::Error> {
        let full_path = self.root_dir.join(file_path);

        // Also try to delete the thumbnail if it exists
        let path = Path::new(file_path);
        let parent = path.parent().unwrap_or(Path::new(""));
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let thumbnail_path = parent.join(format!("{}.thumbnail.jpg", file_stem));
        let full_thumbnail_path = self.root_dir.join(thumbnail_path);

        // Delete the thumbnail if it exists (ignore errors)
        if full_thumbnail_path.exists() {
            let _ = fs::remove_file(full_thumbnail_path);
        }

        // Delete the main file
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
    pub fn cleanup_dir(&self, dir: &Path) -> Result<(), io::Error> {
        if !dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;
        let mut is_empty = true;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.cleanup_dir(&path)?;

                // Check if directory is now empty
                if path.read_dir()?.next().is_none() {
                    fs::remove_dir(path)?;
                } else {
                    is_empty = false;
                }
            } else {
                is_empty = false;
            }
        }

        if is_empty {
            fs::remove_dir(dir)?;
        }

        Ok(())
    }

    // File versioning methods

    /// Create a new version of a file
    pub fn create_version(
        &self,
        file_path: &str,
        comment: Option<String>,
    ) -> Result<VersionMetadata, io::Error> {
        let vm = self.version_manager.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Version manager not initialized")
        })?;

        let full_path = self.get_full_path(file_path);
        vm.create_version(&full_path, comment)
    }

    /// Get version history for a file
    pub fn get_version_history(&self, file_path: &str) -> Result<VersionHistory, io::Error> {
        let vm = self.version_manager.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Version manager not initialized")
        })?;

        let file_id = Path::new(file_path)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path"))?;

        vm.get_version_history(&file_id.to_string_lossy())
    }

    /// Restore a specific version of a file
    pub fn restore_version(&self, file_path: &str, version_id: &str) -> Result<PathBuf, io::Error> {
        let vm = self.version_manager.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Version manager not initialized")
        })?;

        let file_id = Path::new(file_path)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path"))?;

        vm.restore_version(&file_id.to_string_lossy(), version_id)
    }

    /// Delete a specific version of a file
    pub fn delete_version(&self, file_path: &str, version_id: &str) -> Result<(), io::Error> {
        let vm = self.version_manager.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Version manager not initialized")
        })?;

        let file_id = Path::new(file_path)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path"))?;

        vm.delete_version(&file_id.to_string_lossy(), version_id)
    }

    /// Clean up old versions, keeping only the specified number of most recent versions
    pub fn cleanup_versions(&self, file_path: &str, keep_count: usize) -> Result<usize, io::Error> {
        let vm = self.version_manager.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Version manager not initialized")
        })?;

        let file_id = Path::new(file_path)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path"))?;

        vm.cleanup_versions(&file_id.to_string_lossy(), keep_count)
    }
}
