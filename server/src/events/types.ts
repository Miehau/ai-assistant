import type { WaitingFor } from '../domain/types.js'

export interface EventSink {
  emit(event: AgentEvent): void
}

export interface EventSource {
  subscribe(filter: EventFilter): AsyncIterable<AgentEvent>
  subscribeOnce(filter: EventFilter, signal?: AbortSignal): Promise<AgentEvent>
}

// ---------------------------------------------------------------------------
// Typed event payloads — discriminated union keyed on `type`
// ---------------------------------------------------------------------------

export type AgentEvent =
  | AgentStartedEvent
  | AgentCompletedEvent
  | AgentFailedEvent
  | AgentWaitingEvent
  | TurnStartedEvent
  | TurnCompletedEvent
  | ToolStartedEvent
  | ToolCompletedEvent
  | ToolProposedEvent
  | ToolApprovedEvent
  | ToolDeniedEvent
  | StepProposedEvent
  | StepStartedEvent
  | StepCompletedEvent
  | CompanionTextEvent
  | TextDeltaEvent

interface BaseEvent {
  agent_id: string
  session_id: string
  timestamp: number
}

export interface AgentStartedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.AGENT_STARTED
  payload: { task: string; model: string; parentId: string | null; depth: number }
}

export interface AgentCompletedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.AGENT_COMPLETED
  payload: { result: string; parentId: string | null; depth: number }
}

export interface AgentFailedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.AGENT_FAILED
  payload: { error: string; parentId: string | null; depth: number }
}

export interface AgentWaitingEvent extends BaseEvent {
  type: typeof EVENT_TYPES.AGENT_WAITING
  payload: { waitingFor: WaitingFor[]; parentId: string | null; depth: number }
}

export interface TurnStartedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TURN_STARTED
  payload: { turn: number; parentId: string | null; depth: number }
}

export interface TurnCompletedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TURN_COMPLETED
  payload: { turn: number; outcome: string; parentId: string | null; depth: number }
}

export interface ToolStartedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TOOL_STARTED
  payload: { callId: string; name: string; args: Record<string, unknown>; parentId: string | null; depth: number }
}

export interface ToolCompletedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TOOL_COMPLETED
  payload: { callId: string; name: string; success: boolean; output: string; durationMs: number; parentId: string | null; depth: number }
}

export interface ToolProposedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TOOL_PROPOSED
  payload: { callId: string; name: string; args: Record<string, unknown>; parentId: string | null; depth: number }
}

export interface ToolApprovedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TOOL_APPROVED
  payload: { callId: string; name: string }
}

export interface ToolDeniedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TOOL_DENIED
  payload: { callId: string; name: string }
}

export interface StepProposedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.STEP_PROPOSED
  payload: { action: string; turn: number; parentId: string | null; depth: number }
}

export interface StepStartedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.STEP_STARTED
  payload: { stepType: string; turn: number; parentId: string | null; depth: number }
}

export interface StepCompletedEvent extends BaseEvent {
  type: typeof EVENT_TYPES.STEP_COMPLETED
  payload: { stepType: string; turn: number; outcomeType: string; parentId: string | null; depth: number }
}

export interface CompanionTextEvent extends BaseEvent {
  type: typeof EVENT_TYPES.COMPANION_TEXT
  payload: { text: string; parentId: string | null; depth: number }
}

export interface TextDeltaEvent extends BaseEvent {
  type: typeof EVENT_TYPES.TEXT_DELTA
  payload: { text: string; parentId: string | null; depth: number }
}

// ---------------------------------------------------------------------------
// Event filter
// ---------------------------------------------------------------------------

export interface EventFilter {
  agent_id?: string
  session_id?: string
  types?: string[]
}

export const EVENT_TYPES = {
  AGENT_STARTED: 'agent:started',
  AGENT_COMPLETED: 'agent:completed',
  AGENT_FAILED: 'agent:failed',
  AGENT_WAITING: 'agent:waiting',
  TURN_STARTED: 'turn:started',
  TURN_COMPLETED: 'turn:completed',
  TOOL_STARTED: 'tool:started',
  TOOL_COMPLETED: 'tool:completed',
  TOOL_PROPOSED: 'tool:proposed',
  TOOL_APPROVED: 'tool:approved',
  TOOL_DENIED: 'tool:denied',
  STEP_PROPOSED: 'step:proposed',
  STEP_STARTED: 'step:started',
  STEP_COMPLETED: 'step:completed',
  PHASE_CHANGED: 'phase:changed',
  COMPANION_TEXT: 'companion:text',
  TEXT_DELTA: 'text:delta',
} as const
