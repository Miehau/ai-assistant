use crate::files::{FileManager, VersionHistory, VersionMetadata};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionResult {
    pub success: bool,
    pub version: Option<VersionMetadata>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionHistoryResult {
    pub success: bool,
    pub history: Option<VersionHistory>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreVersionResult {
    pub success: bool,
    pub file_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteVersionResult {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CleanupVersionsResult {
    pub success: bool,
    pub deleted_count: usize,
    pub error: Option<String>,
}

/// Create a new version of a file
#[tauri::command]
pub fn create_file_version(
    file_path: String,
    comment: Option<String>,
    file_manager: tauri::State<'_, FileManager>,
) -> VersionResult {
    // FileManager is now passed as a state parameter

    match file_manager.create_version(&file_path, comment) {
        Ok(version) => VersionResult {
            success: true,
            version: Some(version),
            error: None,
        },
        Err(e) => VersionResult {
            success: false,
            version: None,
            error: Some(format!("Failed to create version: {}", e)),
        },
    }
}

/// Get version history for a file
#[tauri::command]
pub fn get_file_version_history(
    file_path: String,
    file_manager: tauri::State<'_, FileManager>,
) -> VersionHistoryResult {
    // FileManager is now passed as a state parameter

    match file_manager.get_version_history(&file_path) {
        Ok(history) => VersionHistoryResult {
            success: true,
            history: Some(history),
            error: None,
        },
        Err(e) => VersionHistoryResult {
            success: false,
            history: None,
            error: Some(format!("Failed to get version history: {}", e)),
        },
    }
}

/// Restore a specific version of a file
#[tauri::command]
pub fn restore_file_version(
    file_path: String,
    version_id: String,
    file_manager: tauri::State<'_, FileManager>,
) -> RestoreVersionResult {
    // FileManager is now passed as a state parameter

    match file_manager.restore_version(&file_path, &version_id) {
        Ok(path) => RestoreVersionResult {
            success: true,
            file_path: Some(path.to_string_lossy().to_string()),
            error: None,
        },
        Err(e) => RestoreVersionResult {
            success: false,
            file_path: None,
            error: Some(format!("Failed to restore version: {}", e)),
        },
    }
}

/// Delete a specific version of a file
#[tauri::command]
pub fn delete_file_version(
    file_path: String,
    version_id: String,
    file_manager: tauri::State<'_, FileManager>,
) -> DeleteVersionResult {
    // FileManager is now passed as a state parameter

    match file_manager.delete_version(&file_path, &version_id) {
        Ok(_) => DeleteVersionResult {
            success: true,
            error: None,
        },
        Err(e) => DeleteVersionResult {
            success: false,
            error: Some(format!("Failed to delete version: {}", e)),
        },
    }
}

/// Clean up old versions, keeping only the specified number of most recent versions
#[tauri::command]
pub fn cleanup_file_versions(
    file_path: String,
    keep_count: usize,
    file_manager: tauri::State<'_, FileManager>,
) -> CleanupVersionsResult {
    // FileManager is now passed as a state parameter

    match file_manager.cleanup_versions(&file_path, keep_count) {
        Ok(count) => CleanupVersionsResult {
            success: true,
            deleted_count: count,
            error: None,
        },
        Err(e) => CleanupVersionsResult {
            success: false,
            deleted_count: 0,
            error: Some(format!("Failed to cleanup versions: {}", e)),
        },
    }
}
