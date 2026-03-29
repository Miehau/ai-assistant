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
 * Not yet implemented in server backend - all methods are stubbed
 */
export class FileVersionService {
  async createVersion(filePath: string, comment?: string): Promise<VersionResult> {
    return { success: false, error: 'Not yet implemented in server backend' };
  }

  async getVersionHistory(filePath: string): Promise<VersionHistoryResult> {
    return { success: false, error: 'Not yet implemented in server backend' };
  }

  async restoreVersion(filePath: string, versionId: string): Promise<RestoreVersionResult> {
    return { success: false, error: 'Not yet implemented in server backend' };
  }

  async deleteVersion(filePath: string, versionId: string): Promise<DeleteVersionResult> {
    return { success: false, error: 'Not yet implemented in server backend' };
  }

  async cleanupVersions(filePath: string, keepCount: number): Promise<CleanupVersionsResult> {
    return { success: false, deleted_count: 0, error: 'Not yet implemented in server backend' };
  }
}

// Create a singleton instance
export const fileVersionService = new FileVersionService();
