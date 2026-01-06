export const AGENT_EVENT_TYPES = {
  MESSAGE_SAVED: 'message.saved',
  CONVERSATION_UPDATED: 'conversation.updated',
  CONVERSATION_DELETED: 'conversation.deleted',
  MESSAGE_USAGE_SAVED: 'message.usage.saved',
  USAGE_UPDATED: 'usage.updated',
  ASSISTANT_STREAM_STARTED: 'assistant.stream.started',
  ASSISTANT_STREAM_CHUNK: 'assistant.stream.chunk',
  ASSISTANT_STREAM_COMPLETED: 'assistant.stream.completed',
} as const;

export type AgentEventType = typeof AGENT_EVENT_TYPES[keyof typeof AGENT_EVENT_TYPES];

export interface MessageSavedPayload {
  conversation_id: string;
  message_id: string;
  role: 'user' | 'assistant';
  content: string;
  attachments: EventAttachment[];
  timestamp_ms: number;
}

export type AgentEventPayloadMap = {
  'message.saved': MessageSavedPayload;
  'conversation.updated': ConversationUpdatedPayload;
  'conversation.deleted': ConversationDeletedPayload;
  'message.usage.saved': MessageUsageSavedPayload;
  'usage.updated': UsageUpdatedPayload;
  'assistant.stream.started': AssistantStreamStartedPayload;
  'assistant.stream.chunk': AssistantStreamChunkPayload;
  'assistant.stream.completed': AssistantStreamCompletedPayload;
};

export interface EventAttachment {
  name: string;
  data: string;
  attachment_type: string;
  description?: string;
  transcript?: string;
}

export interface ConversationUpdatedPayload {
  conversation_id: string;
  name: string;
  timestamp_ms: number;
}

export interface ConversationDeletedPayload {
  conversation_id: string;
  timestamp_ms: number;
}

export interface MessageUsageSavedPayload {
  id: string;
  message_id: string;
  model_name: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  estimated_cost: number;
  timestamp_ms: number;
}

export interface UsageUpdatedPayload {
  conversation_id: string;
  total_prompt_tokens: number;
  total_completion_tokens: number;
  total_tokens: number;
  total_cost: number;
  message_count: number;
  timestamp_ms: number;
}

export interface AssistantStreamStartedPayload {
  conversation_id: string;
  message_id: string;
  timestamp_ms: number;
}

export interface AssistantStreamChunkPayload {
  conversation_id: string;
  message_id: string;
  chunk: string;
  timestamp_ms: number;
}

export interface AssistantStreamCompletedPayload {
  conversation_id: string;
  message_id: string;
  content: string;
  timestamp_ms: number;
}

export interface AgentEvent<T extends AgentEventType = AgentEventType> {
  event_type: T;
  payload: AgentEventPayloadMap[T];
  timestamp_ms: number;
}
