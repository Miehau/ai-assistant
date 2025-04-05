import { invoke } from '@tauri-apps/api/tauri';

export interface VersionMetadata {
  version_id: string;
  created_at: string;
  file_size: number;
  original_path: string;
  version_path: string;
  comment?: string;
}

export interface VersionHistory {
  file_id: string;
  current_version: string;
  versions: VersionMetadata[];
}

export interface VersionResult {
  success: boolean;
  version?: VersionMetadata;
  error?: string;
}

export interface VersionHistoryResult {
  success: boolean;
  history?: VersionHistory;
  error?: string;
}

export interface RestoreVersionResult {
  success: boolean;
  file_path?: string;
  error?: string;
}

export interface DeleteVersionResult {
  success: boolean;
  error?: string;
}

export interface CleanupVersionsResult {
  success: boolean;
  deleted_count: number;
  error?: string;
}

/**
 * Service for managing file versions
 */
export class FileVersionService {
  /**
   * Create a new version of a file
   * @param filePath Path to the file
   * @param comment Optional comment for the version
   */
  async createVersion(filePath: string, comment?: string): Promise<VersionResult> {
    try {
      return await invoke('create_file_version', {
        filePath,
        comment
      }) as VersionResult;
    } catch (error) {
      console.error('Error creating file version:', error);
      return {
        success: false,
        error: `Failed to create version: ${error}`
      };
    }
  }

  /**
   * Get version history for a file
   * @param filePath Path to the file
   */
  async getVersionHistory(filePath: string): Promise<VersionHistoryResult> {
    try {
      return await invoke('get_file_version_history', {
        filePath
      }) as VersionHistoryResult;
    } catch (error) {
      console.error('Error getting file version history:', error);
      return {
        success: false,
        error: `Failed to get version history: ${error}`
      };
    }
  }

  /**
   * Restore a specific version of a file
   * @param filePath Path to the file
   * @param versionId ID of the version to restore
   */
  async restoreVersion(filePath: string, versionId: string): Promise<RestoreVersionResult> {
    try {
      return await invoke('restore_file_version', {
        filePath,
        versionId
      }) as RestoreVersionResult;
    } catch (error) {
      console.error('Error restoring file version:', error);
      return {
        success: false,
        error: `Failed to restore version: ${error}`
      };
    }
  }

  /**
   * Delete a specific version of a file
   * @param filePath Path to the file
   * @param versionId ID of the version to delete
   */
  async deleteVersion(filePath: string, versionId: string): Promise<DeleteVersionResult> {
    try {
      return await invoke('delete_file_version', {
        filePath,
        versionId
      }) as DeleteVersionResult;
    } catch (error) {
      console.error('Error deleting file version:', error);
      return {
        success: false,
        error: `Failed to delete version: ${error}`
      };
    }
  }

  /**
   * Clean up old versions, keeping only the specified number of most recent versions
   * @param filePath Path to the file
   * @param keepCount Number of recent versions to keep
   */
  async cleanupVersions(filePath: string, keepCount: number): Promise<CleanupVersionsResult> {
    try {
      return await invoke('cleanup_file_versions', {
        filePath,
        keepCount
      }) as CleanupVersionsResult;
    } catch (error) {
      console.error('Error cleaning up file versions:', error);
      return {
        success: false,
        deleted_count: 0,
        error: `Failed to cleanup versions: ${error}`
      };
    }
  }
}

// Create a singleton instance
export const fileVersionService = new FileVersionService();
