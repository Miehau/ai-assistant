export interface SystemPrompt {
    id: string;
    name: string;
    content: string;
    created_at: string;
    updated_at: string;
}

export interface Attachment {
    id?: string;
    message_id?: string;
    name: string;
    data: string;
    attachment_url?: string;
    attachment_type: string;
    description?: string;
    created_at?: Date;
}

// For display in UI
export type Message = {
    type: "sent" | "received";
    content: string;
    attachments?: Attachment[];
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
}

export interface ConversationState {
    currentConversationId: string | null;
    currentConversation: Conversation | null;
}

export interface DBMessage {
    role: 'user' | 'assistant';
    content: string;
    attachments?: Attachment[];
    timestamp?: number;
}