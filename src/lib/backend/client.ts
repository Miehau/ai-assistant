/**
 * Backend Client - Centralized abstraction for all backend calls
 *
 * This module provides a unified interface for communicating with the HTTP server backend.
 * Benefits:
 * - Single source of truth for all backend calls
 * - Built-in caching for frequently accessed data
 * - Consistent error handling
 * - Easy to mock for testing
 * - Type-safe API
 */
import { getHttpBackend } from './http-client';
import type { Model } from '$lib/types/models';
import type {
  Conversation,
  SystemPrompt,
  ConversationUsageSummary,
  UsageStatistics,
  UsageBackfillResult,
  Branch,
  MessageTreeNode,
  ConversationTree,
  BranchPath,
  BranchStats,
  DBMessage,
  IntegrationMetadata,
  McpServer,
  CreateMcpServerInput,
  UpdateMcpServerInput,
  IntegrationConnection,
  CreateIntegrationConnectionInput,
  UpdateIntegrationConnectionInput,
  GoogleCalendarListItem,
  OAuthStartResponse,
  OAuthSessionStatus
} from '$lib/types';
import type {
  CustomBackend,
  CreateCustomBackendInput,
  UpdateCustomBackendInput
} from '$lib/types/customBackend';
import type { Attachment, FileMetadata } from '$lib/types/attachments';
import type { ToolMetadata } from '$lib/types/tools';
import type {
  ToolExecutionApprovalScope,
  ToolExecutionProposedPayload
} from '$lib/types/events';

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
 * Backend client class that provides a unified interface for the HTTP server backend.
 * Replaces all Tauri invoke() calls with HTTP client methods.
 */
class BackendClient {
  private cache = new Map<string, CacheEntry<unknown>>();
  private defaultTTL = 60000; // 1 minute

  private get http() {
    return getHttpBackend();
  }

  // ============ Models ============

  async getModels(): Promise<Model[]> {
    return this.cachedFetch('models', () => this.http.listModels()) as Promise<unknown> as Promise<Model[]>;
  }

  async addModel(model: Partial<Model>): Promise<void> {
    this.invalidateCache('models');
    await this.http.addModel(
      model.provider ?? '',
      model.model_name ?? model.name ?? '',
      model.name,
    );
  }

  async toggleModel(model: Pick<Model, 'provider' | 'model_name'>): Promise<void> {
    this.invalidateCache('models');
    // The server PATCH /api/models/:id expects an update object.
    // Since the old Tauri command toggled by provider+model_name, we need to find the model first.
    const models = await this.http.listModels();
    const found = models.find(
      (m) => m.provider === model.provider && (m.name === model.model_name || m.id === model.model_name),
    );
    if (!found || !found.id) {
      throw new Error(`Model not found: ${model.provider}/${model.model_name}`);
    }
    // Toggle is implemented as a PATCH — the server doesn't have a dedicated toggle,
    // so we pass an empty update to trigger server-side toggle logic if it exists.
    await this.http.updateModel(found.id, {});
  }

  async deleteModel(model: Model): Promise<void> {
    this.invalidateCache('models');
    // The HTTP server uses a string ID for models. The Tauri Model type
    // doesn't have an `id` field, so we look up by provider + model_name.
    const models = await this.http.listModels();
    const found = models.find(
      (m) => m.provider === model.provider && (m.name === model.model_name || m.name === model.name),
    );
    if (!found || !found.id) {
      throw new Error(`Model not found for deletion: ${model.provider}/${model.model_name}`);
    }
    await this.http.deleteModel(found.id);
  }

  // ============ API Keys ============

  async getApiKey(provider: string): Promise<string | null> {
    const result = await this.http.getApiKey(provider);
    // Server never returns the actual key, only whether it exists
    return result.exists ? '***' : null;
  }

  async setApiKey(provider: string, apiKey: string): Promise<void> {
    await this.http.setApiKey(provider, apiKey);
  }

  async deleteApiKey(provider: string): Promise<void> {
    await this.http.deleteApiKey(provider);
  }

  // ============ Conversations (mapped to Sessions) ============

  async getConversations(): Promise<Conversation[]> {
    const sessions = await this.http.listSessions();
    return sessions.map((s) => ({
      id: s.id,
      name: s.title ?? '',
      created_at: new Date(s.createdAt).toISOString(),
      updated_at: new Date(s.updatedAt).toISOString(),
    })) as unknown as Conversation[];
  }

  async getOrCreateConversation(conversationId: string | null): Promise<Conversation> {
    if (conversationId) {
      const session = await this.http.getSession(conversationId);
      return {
        id: session.id,
        name: session.title ?? '',
        created_at: new Date(session.createdAt).toISOString(),
        updated_at: new Date(session.updatedAt).toISOString(),
      } as unknown as Conversation;
    }
    // No direct "create" endpoint — send minimal completion to bootstrap a session
    throw new Error('Not yet implemented in server backend');
  }

  async updateConversationName(conversationId: string, name: string): Promise<void> {
    await this.http.updateSession(conversationId, { title: name });
  }

  async deleteConversation(conversationId: string): Promise<void> {
    await this.http.deleteSession(conversationId);
  }

  async getConversationHistory(conversationId: string): Promise<DBMessage[]> {
    const session = await this.http.getSession(conversationId);
    // Map session items to DBMessage shape
    return (session.items ?? []) as unknown as DBMessage[];
  }

  async saveMessage(
    _conversation_id: string,
    _role: 'user' | 'assistant',
    _content: string,
    _attachments: Attachment[] = [],
    _message_id?: string
  ): Promise<string> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Custom Backends ============

  async getCustomBackends(): Promise<CustomBackend[]> {
    throw new Error('Not yet implemented in server backend');
  }

  async getCustomBackend(_id: string): Promise<CustomBackend | null> {
    throw new Error('Not yet implemented in server backend');
  }

  async createCustomBackend(_input: CreateCustomBackendInput): Promise<CustomBackend> {
    throw new Error('Not yet implemented in server backend');
  }

  async updateCustomBackend(_input: UpdateCustomBackendInput): Promise<CustomBackend | null> {
    throw new Error('Not yet implemented in server backend');
  }

  async deleteCustomBackend(_id: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Branches ============

  async createBranch(_conversationId: string, _name: string): Promise<Branch> {
    throw new Error('Not yet implemented in server backend');
  }

  async getOrCreateMainBranch(_conversationId: string): Promise<Branch> {
    throw new Error('Not yet implemented in server backend');
  }

  async createMessageTreeNode(
    _messageId: string,
    _parentMessageId: string | null,
    _branchId: string,
    _isBranchPoint: boolean
  ): Promise<MessageTreeNode> {
    throw new Error('Not yet implemented in server backend');
  }

  async getConversationTree(_conversationId: string): Promise<ConversationTree> {
    throw new Error('Not yet implemented in server backend');
  }

  async getConversationBranches(_conversationId: string): Promise<Branch[]> {
    throw new Error('Not yet implemented in server backend');
  }

  async getBranchPath(_branchId: string): Promise<BranchPath> {
    throw new Error('Not yet implemented in server backend');
  }

  async renameBranch(_branchId: string, _newName: string): Promise<void> {
    throw new Error('Not yet implemented in server backend');
  }

  async deleteBranch(_branchId: string): Promise<void> {
    throw new Error('Not yet implemented in server backend');
  }

  async getBranchStats(_conversationId: string): Promise<BranchStats> {
    throw new Error('Not yet implemented in server backend');
  }

  async createBranchFromMessage(
    _conversationId: string,
    _parentMessageId: string,
    _branchName: string
  ): Promise<Branch> {
    throw new Error('Not yet implemented in server backend');
  }

  async checkMessageTreeConsistency(): Promise<MessageTreeConsistencyCheck> {
    throw new Error('Not yet implemented in server backend');
  }

  async repairMessageTree(): Promise<number> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ System Prompts ============

  async getSystemPrompt(id: string): Promise<SystemPrompt | null> {
    return this.http.getSystemPrompt(id) as Promise<SystemPrompt | null>;
  }

  async getAllSystemPrompts(): Promise<SystemPrompt[]> {
    return this.http.listSystemPrompts() as Promise<SystemPrompt[]>;
  }

  async saveSystemPrompt(name: string, content: string): Promise<SystemPrompt> {
    return this.http.createSystemPrompt(name, content) as Promise<SystemPrompt>;
  }

  async updateSystemPrompt(id: string, name: string, content: string): Promise<SystemPrompt> {
    return this.http.updateSystemPrompt(id, { name, content }) as Promise<SystemPrompt>;
  }

  async deleteSystemPrompt(id: string): Promise<void> {
    await this.http.deleteSystemPrompt(id);
  }

  // ============ Usage ============

  async saveMessageUsage(_input: MessageUsageInput): Promise<void> {
    throw new Error('Not yet implemented in server backend');
  }

  async updateConversationUsage(_conversationId: string): Promise<ConversationUsageSummary> {
    throw new Error('Not yet implemented in server backend');
  }

  async getConversationUsage(_conversationId: string): Promise<ConversationUsageSummary | null> {
    throw new Error('Not yet implemented in server backend');
  }

  async getUsageStatistics(): Promise<UsageStatistics> {
    const stats = await this.http.getUsageStats();
    return stats as unknown as UsageStatistics;
  }

  async getMessageUsage(_messageId: string): Promise<unknown> {
    throw new Error('Not yet implemented in server backend');
  }

  async backfillMessageUsage(_options?: {
    conversation_id?: string;
    default_model?: string;
    dry_run?: boolean;
  }): Promise<UsageBackfillResult> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Preferences ============

  async getPreference(key: string): Promise<string | null> {
    try {
      const result = await this.http.getPreference(key);
      return result.value;
    } catch {
      return null;
    }
  }

  async setPreference(key: string, value: string): Promise<void> {
    await this.http.setPreference(key, value);
  }

  // ============ Integrations ============

  async listIntegrations(): Promise<IntegrationMetadata[]> {
    throw new Error('Not yet implemented in server backend');
  }

  async getIntegrationConnections(): Promise<IntegrationConnection[]> {
    throw new Error('Not yet implemented in server backend');
  }

  async createIntegrationConnection(
    _input: CreateIntegrationConnectionInput
  ): Promise<IntegrationConnection> {
    throw new Error('Not yet implemented in server backend');
  }

  async updateIntegrationConnection(
    _input: UpdateIntegrationConnectionInput
  ): Promise<IntegrationConnection | null> {
    throw new Error('Not yet implemented in server backend');
  }

  async deleteIntegrationConnection(_id: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  async testIntegrationConnection(_id: string): Promise<{ ok: boolean; status: number }> {
    throw new Error('Not yet implemented in server backend');
  }

  async startGoogleOAuth(_integrationId: string): Promise<OAuthStartResponse> {
    throw new Error('Not yet implemented in server backend');
  }

  async listGoogleCalendars(_connectionId: string): Promise<GoogleCalendarListItem[]> {
    throw new Error('Not yet implemented in server backend');
  }

  async getOauthSession(_sessionId: string): Promise<OAuthSessionStatus> {
    throw new Error('Not yet implemented in server backend');
  }

  async cancelOauthSession(_sessionId: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ MCP Servers ============

  async getMcpServers(): Promise<McpServer[]> {
    throw new Error('Not yet implemented in server backend');
  }

  async getMcpServer(_id: string): Promise<McpServer | null> {
    throw new Error('Not yet implemented in server backend');
  }

  async createMcpServer(_input: CreateMcpServerInput): Promise<McpServer> {
    throw new Error('Not yet implemented in server backend');
  }

  async updateMcpServer(_input: UpdateMcpServerInput): Promise<McpServer | null> {
    throw new Error('Not yet implemented in server backend');
  }

  async deleteMcpServer(_id: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  async testMcpServer(_id: string): Promise<{ ok: boolean; status: number }> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Tools ============

  async listTools(): Promise<ToolMetadata[]> {
    return this.http.listTools() as Promise<unknown> as Promise<ToolMetadata[]>;
  }

  async listPendingToolApprovals(): Promise<ToolExecutionProposedPayload[]> {
    throw new Error('Not yet implemented in server backend');
  }

  async resolveToolExecutionApproval(
    _approvalId: string,
    _approved: boolean,
    _scope?: ToolExecutionApprovalScope
  ): Promise<void> {
    throw new Error('Not yet implemented in server backend');
  }

  async setToolApprovalOverride(
    _toolName: string,
    _requiresApproval: boolean | null
  ): Promise<void> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Files ============

  async uploadFile(
    _fileData: string,
    _fileName: string,
    _mimeType: string,
    _conversationId: string,
    _messageId: string
  ): Promise<FileUploadResult> {
    throw new Error('Not yet implemented in server backend');
  }

  async uploadFileFromPath(
    _filePath: string,
    _fileName: string,
    _mimeType: string,
    _conversationId: string,
    _messageId: string
  ): Promise<FileUploadResult> {
    throw new Error('Not yet implemented in server backend');
  }

  async getFile(_filePath: string, _asBase64: boolean = true): Promise<string> {
    throw new Error('Not yet implemented in server backend');
  }

  async deleteFile(_filePath: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  async cleanupEmptyDirectories(): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Image Processing ============

  async getImageThumbnail(_filePath: string): Promise<string> {
    throw new Error('Not yet implemented in server backend');
  }

  async optimizeImage(
    _filePath: string,
    _maxWidth: number = 1200,
    _maxHeight: number = 1200,
    _quality: number = 80
  ): Promise<string> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Audio Processing ============

  async validateAudio(_fileData: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  async extractAudioMetadata(_filePath: string): Promise<unknown> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Text Processing ============

  async validateText(_fileData: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  async extractTextMetadata(_filePath: string): Promise<unknown> {
    throw new Error('Not yet implemented in server backend');
  }

  async extractCodeBlocks(_filePath: string): Promise<[string, string][]> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ File Versioning ============

  async createFileVersion(_filePath: string, _comment?: string): Promise<VersionResult> {
    throw new Error('Not yet implemented in server backend');
  }

  async getFileVersionHistory(_filePath: string): Promise<VersionHistoryResult> {
    throw new Error('Not yet implemented in server backend');
  }

  async restoreFileVersion(_filePath: string, _versionId: string): Promise<RestoreVersionResult> {
    throw new Error('Not yet implemented in server backend');
  }

  async deleteFileVersion(_filePath: string, _versionId: string): Promise<DeleteVersionResult> {
    throw new Error('Not yet implemented in server backend');
  }

  async cleanupFileVersions(_filePath: string, _keepCount: number): Promise<CleanupVersionsResult> {
    throw new Error('Not yet implemented in server backend');
  }

  // ============ Cache Helpers ============

  private async cachedFetch<T>(key: string, fetcher: () => Promise<T>, ttl = this.defaultTTL): Promise<T> {
    const cached = this.cache.get(key);

    if (cached && Date.now() - cached.timestamp < ttl) {
      return cached.data as T;
    }

    const data = await fetcher();
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
