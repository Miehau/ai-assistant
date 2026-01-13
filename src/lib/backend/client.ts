/**
 * Backend Client - Centralized abstraction for all Tauri invoke calls
 *
 * This module provides a unified interface for communicating with the Rust backend.
 * Benefits:
 * - Single source of truth for all backend calls
 * - Built-in caching for frequently accessed data
 * - Consistent error handling
 * - Easy to mock for testing
 * - Type-safe API
 */
import { invoke } from '@tauri-apps/api/tauri';
import type { Model } from '$lib/types/models';
import type {
  Conversation,
  SystemPrompt,
  ConversationUsageSummary,
  UsageStatistics,
  Branch,
  MessageTreeNode,
  ConversationTree,
  BranchPath,
  BranchStats,
  DBMessage
} from '$lib/types';
import type {
  CustomBackend,
  CreateCustomBackendInput,
  UpdateCustomBackendInput
} from '$lib/types/customBackend';
import type { Attachment, FileMetadata } from '$lib/types/attachments';
import type { ToolMetadata } from '$lib/types/tools';

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

interface FileUploadResult {
  metadata: FileMetadata;
  success: boolean;
  error?: string;
}

interface VersionMetadata {
  version_id: string;
  created_at: string;
  file_size: number;
  original_path: string;
  version_path: string;
  comment?: string;
}

interface VersionHistory {
  file_id: string;
  current_version: string;
  versions: VersionMetadata[];
}

interface VersionResult {
  success: boolean;
  version?: VersionMetadata;
  error?: string;
}

interface VersionHistoryResult {
  success: boolean;
  history?: VersionHistory;
  error?: string;
}

interface RestoreVersionResult {
  success: boolean;
  file_path?: string;
  error?: string;
}

interface DeleteVersionResult {
  success: boolean;
  error?: string;
}

interface CleanupVersionsResult {
  success: boolean;
  deleted_count: number;
  error?: string;
}

interface MessageUsageInput {
  message_id: string;
  model_name: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  estimated_cost: number;
}

interface MessageTreeConsistencyCheck {
  orphaned_messages: string[];
  orphaned_count: number;
  is_consistent: boolean;
  warnings: string[];
}

/**
 * Backend client class that provides a unified interface for all Tauri commands
 */
class BackendClient {
  private cache = new Map<string, CacheEntry<unknown>>();
  private defaultTTL = 60000; // 1 minute

  // ============ Models ============

  async getModels(): Promise<Model[]> {
    return this.cachedInvoke('get_models', {});
  }

  async addModel(model: Partial<Model>): Promise<void> {
    this.invalidateCache('get_models');
    return invoke('add_model', { model });
  }

  async toggleModel(model: Pick<Model, 'provider' | 'model_name'>): Promise<void> {
    this.invalidateCache('get_models');
    return invoke('toggle_model', { model });
  }

  async deleteModel(model: Model): Promise<void> {
    this.invalidateCache('get_models');
    return invoke('delete_model', { model });
  }

  // ============ API Keys ============

  async getApiKey(provider: string): Promise<string | null> {
    return invoke('get_api_key', { provider });
  }

  async setApiKey(provider: string, apiKey: string): Promise<void> {
    return invoke('set_api_key', { provider, apiKey });
  }

  async deleteApiKey(provider: string): Promise<void> {
    return invoke('delete_api_key', { provider });
  }

  // ============ Conversations ============

  async getConversations(): Promise<Conversation[]> {
    return invoke('get_conversations', {});
  }

  async getOrCreateConversation(conversationId: string | null): Promise<Conversation> {
    return invoke('get_or_create_conversation', { conversationId });
  }

  async updateConversationName(conversationId: string, name: string): Promise<void> {
    return invoke('update_conversation_name', { conversationId, name });
  }

  async deleteConversation(conversationId: string): Promise<void> {
    return invoke('delete_conversation', { conversationId });
  }

  async getConversationHistory(conversationId: string): Promise<DBMessage[]> {
    return invoke('get_conversation_history', { conversationId });
  }

  async saveMessage(
    conversation_id: string,
    role: 'user' | 'assistant',
    content: string,
    attachments: Attachment[] = [],
    message_id?: string
  ): Promise<string> {
    return invoke('save_message', {
      conversation_id,
      role,
      content,
      attachments,
      message_id
    });
  }

  // ============ Custom Backends ============

  async getCustomBackends(): Promise<CustomBackend[]> {
    return this.cachedInvoke('get_custom_backends', {});
  }

  async getCustomBackend(id: string): Promise<CustomBackend | null> {
    return invoke('get_custom_backend', { id });
  }

  async createCustomBackend(input: CreateCustomBackendInput): Promise<CustomBackend> {
    this.invalidateCache('get_custom_backends');
    return invoke('create_custom_backend', { input });
  }

  async updateCustomBackend(input: UpdateCustomBackendInput): Promise<CustomBackend | null> {
    this.invalidateCache('get_custom_backends');
    return invoke('update_custom_backend', { input });
  }

  async deleteCustomBackend(id: string): Promise<boolean> {
    this.invalidateCache('get_custom_backends');
    return invoke('delete_custom_backend', { id });
  }

  // ============ Branches ============

  async createBranch(conversationId: string, name: string): Promise<Branch> {
    return invoke('create_branch', { conversationId, name });
  }

  async getOrCreateMainBranch(conversationId: string): Promise<Branch> {
    return invoke('get_or_create_main_branch', { conversationId });
  }

  async createMessageTreeNode(
    messageId: string,
    parentMessageId: string | null,
    branchId: string,
    isBranchPoint: boolean
  ): Promise<MessageTreeNode> {
    return invoke('create_message_tree_node', {
      messageId,
      parentMessageId,
      branchId,
      isBranchPoint
    });
  }

  async getConversationTree(conversationId: string): Promise<ConversationTree> {
    return invoke('get_conversation_tree', { conversationId });
  }

  async getConversationBranches(conversationId: string): Promise<Branch[]> {
    return invoke('get_conversation_branches', { conversationId });
  }

  async getBranchPath(branchId: string): Promise<BranchPath> {
    return invoke('get_branch_path', { branchId });
  }

  async renameBranch(branchId: string, newName: string): Promise<void> {
    return invoke('rename_branch', { branchId, newName });
  }

  async deleteBranch(branchId: string): Promise<void> {
    return invoke('delete_branch', { branchId });
  }

  async getBranchStats(conversationId: string): Promise<BranchStats> {
    return invoke('get_branch_stats', { conversationId });
  }

  async createBranchFromMessage(
    conversationId: string,
    parentMessageId: string,
    branchName: string
  ): Promise<Branch> {
    return invoke('create_branch_from_message', {
      conversationId,
      parentMessageId,
      branchName
    });
  }

  async checkMessageTreeConsistency(): Promise<MessageTreeConsistencyCheck> {
    return invoke('check_message_tree_consistency');
  }

  async repairMessageTree(): Promise<number> {
    return invoke('repair_message_tree');
  }

  // ============ System Prompts ============

  async getSystemPrompt(id: string): Promise<SystemPrompt | null> {
    return invoke('get_system_prompt', { id });
  }

  async getAllSystemPrompts(): Promise<SystemPrompt[]> {
    return invoke('get_all_system_prompts', {});
  }

  async saveSystemPrompt(name: string, content: string): Promise<SystemPrompt> {
    return invoke('save_system_prompt', { name, content });
  }

  async updateSystemPrompt(id: string, name: string, content: string): Promise<SystemPrompt> {
    return invoke('update_system_prompt', { id, name, content });
  }

  async deleteSystemPrompt(id: string): Promise<void> {
    return invoke('delete_system_prompt', { id });
  }

  // ============ Usage ============

  async saveMessageUsage(input: MessageUsageInput): Promise<void> {
    return invoke('save_message_usage', { input });
  }

  async updateConversationUsage(conversationId: string): Promise<ConversationUsageSummary> {
    return invoke('update_conversation_usage', { conversationId });
  }

  async getConversationUsage(conversationId: string): Promise<ConversationUsageSummary | null> {
    return invoke('get_conversation_usage', { conversationId });
  }

  async getUsageStatistics(): Promise<UsageStatistics> {
    return invoke('get_usage_statistics', {});
  }

  async getMessageUsage(messageId: string): Promise<unknown> {
    return invoke('get_message_usage', { messageId });
  }

  // ============ Preferences ============

  async getPreference(key: string): Promise<string | null> {
    return invoke('get_preference', { key });
  }

  async setPreference(key: string, value: string): Promise<void> {
    return invoke('set_preference', { key, value });
  }

  // ============ Tools ============

  async listTools(): Promise<ToolMetadata[]> {
    return invoke('list_tools', {});
  }

  // ============ Files ============

  async uploadFile(
    fileData: string,
    fileName: string,
    mimeType: string,
    conversationId: string,
    messageId: string
  ): Promise<FileUploadResult> {
    return invoke('upload_file', {
      payload: {
        file_data: fileData,
        file_name: fileName,
        mime_type: mimeType,
        conversation_id: conversationId,
        message_id: messageId
      }
    });
  }

  async uploadFileFromPath(
    filePath: string,
    fileName: string,
    mimeType: string,
    conversationId: string,
    messageId: string
  ): Promise<FileUploadResult> {
    return invoke('upload_file_from_path', {
      payload: {
        file_path: filePath,
        file_name: fileName,
        mime_type: mimeType,
        conversation_id: conversationId,
        message_id: messageId
      }
    });
  }

  async getFile(filePath: string, asBase64: boolean = true): Promise<string> {
    return invoke('get_file', { filePath, asBase64 });
  }

  async deleteFile(filePath: string): Promise<boolean> {
    return invoke('delete_file', { filePath });
  }

  async cleanupEmptyDirectories(): Promise<boolean> {
    return invoke('cleanup_empty_directories');
  }

  // ============ Image Processing ============

  async getImageThumbnail(filePath: string): Promise<string> {
    return invoke('get_image_thumbnail', { filePath });
  }

  async optimizeImage(
    filePath: string,
    maxWidth: number = 1200,
    maxHeight: number = 1200,
    quality: number = 80
  ): Promise<string> {
    return invoke('optimize_image', {
      filePath,
      options: { max_width: maxWidth, max_height: maxHeight, quality }
    });
  }

  // ============ Audio Processing ============

  async validateAudio(fileData: string): Promise<boolean> {
    return invoke('validate_audio', { fileData });
  }

  async extractAudioMetadata(filePath: string): Promise<unknown> {
    return invoke('extract_audio_metadata', { filePath });
  }

  // ============ Text Processing ============

  async validateText(fileData: string): Promise<boolean> {
    return invoke('validate_text', { fileData });
  }

  async extractTextMetadata(filePath: string): Promise<unknown> {
    return invoke('extract_text_metadata', { filePath });
  }

  async extractCodeBlocks(filePath: string): Promise<[string, string][]> {
    return invoke('extract_code_blocks', { filePath });
  }

  // ============ File Versioning ============

  async createFileVersion(filePath: string, comment?: string): Promise<VersionResult> {
    return invoke('create_file_version', { filePath, comment });
  }

  async getFileVersionHistory(filePath: string): Promise<VersionHistoryResult> {
    return invoke('get_file_version_history', { filePath });
  }

  async restoreFileVersion(filePath: string, versionId: string): Promise<RestoreVersionResult> {
    return invoke('restore_file_version', { filePath, versionId });
  }

  async deleteFileVersion(filePath: string, versionId: string): Promise<DeleteVersionResult> {
    return invoke('delete_file_version', { filePath, versionId });
  }

  async cleanupFileVersions(filePath: string, keepCount: number): Promise<CleanupVersionsResult> {
    return invoke('cleanup_file_versions', { filePath, keepCount });
  }

  // ============ Cache Helpers ============

  private async cachedInvoke<T>(cmd: string, args: Record<string, unknown>, ttl = this.defaultTTL): Promise<T> {
    const key = `${cmd}:${JSON.stringify(args)}`;
    const cached = this.cache.get(key);

    if (cached && Date.now() - cached.timestamp < ttl) {
      return cached.data as T;
    }

    const data = await invoke<T>(cmd, args);
    this.cache.set(key, { data, timestamp: Date.now() });
    return data;
  }

  private invalidateCache(prefix: string): void {
    for (const key of this.cache.keys()) {
      if (key.startsWith(prefix)) {
        this.cache.delete(key);
      }
    }
  }

  /**
   * Clear all cached data
   */
  clearCache(): void {
    this.cache.clear();
  }

  /**
   * Invalidate cache for a specific command
   */
  invalidateCacheForCommand(cmd: string): void {
    this.invalidateCache(cmd);
  }
}

/**
 * Singleton backend client instance
 */
export const backend = new BackendClient();

/**
 * Export class for testing purposes
 */
export { BackendClient };
