export const AGENT_EVENT_TYPES = {
  MESSAGE_SAVED: 'message.saved',
  CONVERSATION_UPDATED: 'conversation.updated',
  CONVERSATION_DELETED: 'conversation.deleted',
  MESSAGE_USAGE_SAVED: 'message.usage.saved',
  USAGE_UPDATED: 'usage.updated',
  ASSISTANT_STREAM_STARTED: 'assistant.stream.started',
  ASSISTANT_STREAM_CHUNK: 'assistant.stream.chunk',
  ASSISTANT_STREAM_COMPLETED: 'assistant.stream.completed',
  TOOL_EXECUTION_STARTED: 'tool.execution.started',
  TOOL_EXECUTION_COMPLETED: 'tool.execution.completed',
  TOOL_EXECUTION_PROPOSED: 'tool.execution.proposed',
  TOOL_EXECUTION_APPROVED: 'tool.execution.approved',
  TOOL_EXECUTION_DENIED: 'tool.execution.denied',
  AGENT_PHASE_CHANGED: 'agent.phase.changed',
  AGENT_PLAN_CREATED: 'agent.plan.created',
  AGENT_PLAN_ADJUSTED: 'agent.plan.adjusted',
  AGENT_STEP_PROPOSED: 'agent.step.proposed',
  AGENT_STEP_STARTED: 'agent.step.started',
  AGENT_STEP_COMPLETED: 'agent.step.completed',
  AGENT_COMPLETED: 'agent.completed',
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
  'tool.execution.started': ToolExecutionStartedPayload;
  'tool.execution.completed': ToolExecutionCompletedPayload;
  'tool.execution.proposed': ToolExecutionProposedPayload;
  'tool.execution.approved': ToolExecutionDecisionPayload;
  'tool.execution.denied': ToolExecutionDecisionPayload;
  'agent.phase.changed': AgentPhaseChangedPayload;
  'agent.plan.created': AgentPlanPayload;
  'agent.plan.adjusted': AgentPlanPayload;
  'agent.step.proposed': AgentStepProposedPayload;
  'agent.step.started': AgentStepStartedPayload;
  'agent.step.completed': AgentStepCompletedPayload;
  'agent.completed': AgentCompletedPayload;
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

export interface ToolExecutionStartedPayload {
  execution_id: string;
  tool_name: string;
  args: Record<string, unknown>;
  requires_approval: boolean;
  iteration: number;
  conversation_id?: string;
  message_id?: string;
  timestamp_ms: number;
}

export interface ToolExecutionProposedPayload {
  execution_id: string;
  approval_id: string;
  tool_name: string;
  args: Record<string, unknown>;
  preview?: unknown;
  iteration: number;
  conversation_id?: string;
  message_id?: string;
  timestamp_ms: number;
}

export interface ToolExecutionDecisionPayload {
  execution_id: string;
  approval_id: string;
  tool_name: string;
  iteration: number;
  conversation_id?: string;
  message_id?: string;
  timestamp_ms: number;
}

export interface ToolExecutionCompletedPayload {
  execution_id: string;
  tool_name: string;
  result?: unknown;
  success: boolean;
  error?: string;
  duration_ms: number;
  iteration: number;
  conversation_id?: string;
  message_id?: string;
  timestamp_ms: number;
}

export interface AgentPhaseChangedPayload {
  session_id: string;
  phase: unknown;
}

export interface AgentPlanPayload {
  session_id: string;
  plan: unknown;
}

export interface AgentStepProposedPayload {
  session_id: string;
  step: unknown;
  risk: string;
  approval_id?: string | null;
  preview?: unknown;
}

export interface AgentStepStartedPayload {
  session_id: string;
  step_id: string;
}

export interface AgentStepCompletedPayload {
  session_id: string;
  step_id: string;
  success: boolean;
  result?: unknown;
  error?: string | null;
}

export interface AgentCompletedPayload {
  session_id: string;
  response: string;
}

export interface AgentEvent<T extends AgentEventType = AgentEventType> {
  event_type: T;
  payload: AgentEventPayloadMap[T];
  timestamp_ms: number;
}
