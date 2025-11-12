export interface SystemPrompt {
    id: string;
    name: string;
    content: string;
    created_at: string;
    updated_at: string;
}

export interface FileMetadata {
    id: string;
    name: string;
    path: string;
    mime_type: string;
    size_bytes: number;
    created_at: string;
    updated_at: string;
    thumbnail_path?: string;
    metadata?: any;
}

export interface Attachment {
    id?: string;
    message_id?: string;
    name: string;
    data: string;
    attachment_url?: string;
    attachment_type: "image" | "audio" | "text";
    description?: string;
    created_at?: Date;
    transcript?: string;
    // New fields for file-based attachments
    file_path?: string;
    file_metadata?: FileMetadata;
}

// For display in UI
export type Message = {
    id?: string; // Optional message ID for branching
    type: "sent" | "received";
    content: string;
    attachments?: Attachment[];
    model?: string; // Optional model name for display
};

// For API communication
export type APIMessage = {
    role: 'user' | 'assistant' | 'system';
    content: string;
    attachments?: Attachment[];
};

export interface Conversation {
    id: string;
    name: string;
    created_at: string;
}

export interface ConversationState {
    currentConversationId: string | null;
    currentConversation: Conversation | null;
}

export interface DBMessage {
    id?: string; // Message ID from database
    role: 'user' | 'assistant';
    content: string;
    attachments?: Attachment[];
    timestamp?: number;
}

export interface MessageUsage {
    id: string;
    message_id: string;
    model_name: string;
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
    estimated_cost: number;
    created_at: string;
}

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

// Branch management types
export interface Branch {
    id: string;
    conversation_id: string;
    name: string;
    created_at: string;
}

export interface MessageTreeNode {
    message_id: string;
    parent_message_id: string | null;
    branch_id: string;
    branch_point: boolean;
    created_at: string;
}

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

// Extended message type with tree information
export interface MessageWithTree extends Message {
    id?: string;
    has_branches?: boolean;
    branch_count?: number;
    is_branch_point?: boolean;
}

// Branch state for UI
export interface BranchState {
    currentBranchId: string | null;
    branches: Branch[];
    tree: ConversationTree | null;
    selectedPath: string[]; // Array of message IDs in current branch path
}