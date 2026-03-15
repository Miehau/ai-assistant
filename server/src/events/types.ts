export interface EventSink {
  emit(event: AgentEvent): void
}

export interface EventSource {
  subscribe(filter: EventFilter): AsyncIterable<AgentEvent>
  subscribeOnce(filter: EventFilter): Promise<AgentEvent>
}

export interface AgentEvent {
  type: string
  agent_id: string
  session_id: string
  payload: unknown
  timestamp: number
}

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
