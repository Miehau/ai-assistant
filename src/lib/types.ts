export interface SystemPrompt {
    id: string;
    name: string;
    content: string;
    created_at: string;
    updated_at: string;
}

// Import types first so we can create aliases
import type {
    DisplayMessage,
    APIMessage,
    DBMessage,
    MessageWithBranch as MessageWithBranchType,
    MessageUsage as MessageUsageType,
    MessageTreeNode as MessageTreeNodeType,
    BaseMessage,
    ToolCallRecord,
    ToolExecutionDbRecord
} from './types/message';
import type { Attachment, FileMetadata } from './types/attachments';
import type { AgentEvent, AgentEventType } from './types/events';
import type { ToolMetadata } from './types/tools';
import type { IntegrationMetadata } from './types/integrations';
import type { McpServer, CreateMcpServerInput, UpdateMcpServerInput } from './types/mcpServer';
import type {
    IntegrationConnection,
    CreateIntegrationConnectionInput,
    UpdateIntegrationConnectionInput
} from './types/integrationConnection';
import type { OAuthStartResponse, OAuthSessionStatus } from './types/oauth';

// Re-export everything
export type {
    Attachment,
    FileMetadata,
    DisplayMessage,
    APIMessage,
    DBMessage,
    BaseMessage,
    AgentEvent,
    AgentEventType,
    ToolCallRecord,
    ToolExecutionDbRecord,
    ToolMetadata,
    IntegrationMetadata,
    McpServer,
    CreateMcpServerInput,
    UpdateMcpServerInput,
    IntegrationConnection,
    CreateIntegrationConnectionInput,
    UpdateIntegrationConnectionInput,
    OAuthStartResponse,
    OAuthSessionStatus
};

// Re-export with original names
export type MessageWithBranch = MessageWithBranchType;
export type MessageUsage = MessageUsageType;
export type MessageTreeNode = MessageTreeNodeType;

// Export converters for easy access
export {
    toDisplayMessage,
    toAPIMessage,
    toDBMessage,
    dbToAPIMessage,
    apiToDisplayMessage,
    toDisplayMessages,
    toAPIMessages,
    dbToAPIMessages
} from './types/converters';

/**
 * @deprecated Use DisplayMessage instead
 * Kept for backward compatibility during migration
 */
export type Message = DisplayMessage;

export interface Conversation {
    id: string;
    name: string;
    created_at: string;
}

export interface ConversationState {
    currentConversationId: string | null;
    currentConversation: Conversation | null;
}

// MessageUsage kept here as it's not strictly a message type
// but usage tracking metadata

export interface ConversationUsageSummary {
    conversation_id: string;
    total_prompt_tokens: number;
    total_completion_tokens: number;
    total_tokens: number;
    total_cost: number;
    message_count: number;
    last_updated: string;
}

export interface UsageStatistics {
    total_messages: number;
    total_tokens: number;
    total_cost: number;
    by_model: ModelUsage[];
    by_date: DailyUsage[];
    by_model_date: DailyModelUsage[];
}

export interface ModelUsage {
    model_name: string;
    message_count: number;
    total_tokens: number;
    total_cost: number;
}

export interface DailyUsage {
    date: string;
    message_count: number;
    total_tokens: number;
    total_cost: number;
}

export interface DailyModelUsage {
    date: string;
    model_name: string;
    message_count: number;
    total_tokens: number;
    total_cost: number;
}

export interface UsageBackfillResult {
    conversations_scanned: number;
    messages_checked: number;
    messages_backfilled: number;
    conversations_updated: number;
    fallback_model_used: number;
}

// Branch management types
export interface Branch {
    id: string;
    conversation_id: string;
    name: string;
    created_at: string;
}

// MessageTreeNode now exported from types/message.ts

export interface ConversationTree {
    conversation_id: string;
    branches: Branch[];
    nodes: MessageTreeNode[];
    messages: DBMessage[];
}

export interface BranchPath {
    branch: Branch;
    messages: DBMessage[];
}

export interface BranchStats {
    conversation_id: string;
    total_branches: number;
    total_messages: number;
    branch_points: number;
}

/**
 * @deprecated Use MessageWithBranch from types/message.ts instead
 * Kept for backward compatibility
 */
export type MessageWithTree = MessageWithBranch;

// Branch state for UI
export interface BranchState {
    currentBranchId: string | null;
    branches: Branch[];
    tree: ConversationTree | null;
    selectedPath: string[]; // Array of message IDs in current branch path
}
