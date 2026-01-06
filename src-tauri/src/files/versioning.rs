use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub version_id: String,
    pub created_at: DateTime<Utc>,
    pub file_size: u64,
    pub original_path: String,
    pub version_path: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    pub file_id: String,
    pub current_version: String,
    pub versions: Vec<VersionMetadata>,
}

pub struct VersionManager {
    versions_dir: PathBuf,
}

impl VersionManager {
    pub fn new(root_dir: &Path) -> Result<Self, io::Error> {
        let versions_dir = root_dir.join("versions");
        fs::create_dir_all(&versions_dir)?;
        
        Ok(Self { versions_dir })
    }
    
    /// Create a new version of a file
    pub fn create_version(&self, file_path: &Path, comment: Option<String>) -> Result<VersionMetadata, io::Error> {
        // Get file info
        let file_metadata = fs::metadata(file_path)?;
        let file_size = file_metadata.len();
        
        // Generate version ID (timestamp-based)
        let now = Utc::now();
        let version_id = format!("v-{}", now.timestamp());
        
        // Create version directory if it doesn't exist
        let file_id = file_path.file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path"))?
            .to_string_lossy()
            .to_string();
            
        let version_dir = self.versions_dir.join(&file_id);
        fs::create_dir_all(&version_dir)?;
        
        // Create version path
        let extension = file_path.extension()
            .map(|ext| format!(".{}", ext.to_string_lossy()))
            .unwrap_or_default();
            
        let version_filename = format!("{}{}", version_id, extension);
        let version_path = version_dir.join(&version_filename);
        
        // Copy file to version directory
        fs::copy(file_path, &version_path)?;
        
        // Create version metadata
        let version_metadata = VersionMetadata {
            version_id,
            created_at: now,
            file_size,
            original_path: file_path.to_string_lossy().to_string(),
            version_path: version_path.to_string_lossy().to_string(),
            comment,
        };
        
        // Update version history
        self.update_version_history(&file_id, &version_metadata)?;
        
        Ok(version_metadata)
    }
    
    /// Get version history for a file
    pub fn get_version_history(&self, file_id: &str) -> Result<VersionHistory, io::Error> {
        let history_path = self.versions_dir.join(file_id).join("history.json");
        
        if !history_path.exists() {
            return Ok(VersionHistory {
                file_id: file_id.to_string(),
                current_version: "".to_string(),
                versions: Vec::new(),
            });
        }
        
        let history_content = fs::read_to_string(history_path)?;
        let history: VersionHistory = serde_json::from_str(&history_content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            
        Ok(history)
    }
    
    /// Update version history for a file
    fn update_version_history(&self, file_id: &str, version: &VersionMetadata) -> Result<(), io::Error> {
        let mut history = self.get_version_history(file_id)?;
        
        // Update current version
        history.current_version = version.version_id.clone();
        
        // Add new version to history
        history.versions.push(version.clone());
        
        // Sort versions by creation date (newest first)
        history.versions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        // Save updated history
        let history_path = self.versions_dir.join(file_id).join("history.json");
        let history_content = serde_json::to_string_pretty(&history)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            
        fs::write(history_path, history_content)?;
        
        Ok(())
    }
    
    /// Restore a specific version of a file
    pub fn restore_version(&self, file_id: &str, version_id: &str) -> Result<PathBuf, io::Error> {
        let history = self.get_version_history(file_id)?;
        
        // Find the requested version
        let version = history.versions.iter()
            .find(|v| v.version_id == version_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Version not found"))?;
            
        // Get the original file path
        let original_path = Path::new(&version.original_path);
        
        // Create a new version of the current file before restoring
        if let Ok(current_version) = self.create_version(original_path, Some("Auto-backup before restore".to_string())) {
            println!("Created backup version {} before restore", current_version.version_id);
        }
        
        // Copy the version file to the original location
        fs::copy(&version.version_path, &version.original_path)?;
        
        // Update current version in history
        let mut updated_history = history.clone();
        updated_history.current_version = version_id.to_string();
        
        // Save updated history
        let history_path = self.versions_dir.join(file_id).join("history.json");
        let history_content = serde_json::to_string_pretty(&updated_history)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            
        fs::write(history_path, history_content)?;
        
        Ok(Path::new(&version.original_path).to_path_buf())
    }
    
    /// Delete a specific version of a file
    pub fn delete_version(&self, file_id: &str, version_id: &str) -> Result<(), io::Error> {
        let history = self.get_version_history(file_id)?;
        
        // Find the requested version
        let version = history.versions.iter()
            .find(|v| v.version_id == version_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Version not found"))?;
            
        // Cannot delete current version
        if history.current_version == version_id {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot delete current version"));
        }
        
        // Delete version file
        if Path::new(&version.version_path).exists() {
            fs::remove_file(&version.version_path)?;
        }
        
        // Update history
        let mut updated_history = history.clone();
        updated_history.versions.retain(|v| v.version_id != version_id);
        
        // Save updated history
        let history_path = self.versions_dir.join(file_id).join("history.json");
        let history_content = serde_json::to_string_pretty(&updated_history)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            
        fs::write(history_path, history_content)?;
        
        Ok(())
    }
    
    /// Clean up old versions, keeping only the specified number of most recent versions
    pub fn cleanup_versions(&self, file_id: &str, keep_count: usize) -> Result<usize, io::Error> {
        let history = self.get_version_history(file_id)?;
        
        // If we have fewer versions than the keep count, nothing to do
        if history.versions.len() <= keep_count {
            return Ok(0);
        }
        
        // Determine which versions to delete (oldest first, but never delete current version)
        let current_version_id = history.current_version.clone();
        let versions_to_delete: Vec<VersionMetadata> = history.versions.iter()
            .filter(|v| v.version_id != current_version_id)
            .skip(keep_count - 1)  // Keep the most recent versions
            .cloned()
            .collect();
            
        let mut deleted_count = 0;
        
        // Delete each version
        for version in versions_to_delete {
            if let Ok(()) = self.delete_version(file_id, &version.version_id) {
                deleted_count += 1;
            }
        }
        
        Ok(deleted_count)
    }
}
