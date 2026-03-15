import type { Agent, WaitingFor } from '../domain/types.js'
import type { LLMProvider } from '../providers/types.js'
import type { ToolExecutor } from '../tools/types.js'
import type { AgentRepository, ItemRepository, ToolOutputRepository } from '../repositories/types.js'
import type { EventSink } from '../events/types.js'

export type ControllerAction =
  | { action: 'next_step'; thinking?: unknown; step_type?: string; tool?: string; tools?: ToolCallSpec[]; args?: Record<string, unknown>; message?: string; question?: string; context?: string; save?: boolean }
  | { action: 'complete'; message: string }
  | { action: 'guardrail_stop'; reason: string; message?: string }
  | { action: 'ask_user'; question: string; context?: string }

export interface ToolCallSpec {
  tool: string
  args: Record<string, unknown>
  save?: boolean
}

export type StepExecutionOutcome =
  | { type: 'continue' }
  | { type: 'complete'; response: string }
  | { type: 'waiting'; waiting_for: WaitingFor[] }

export interface OrchestratorDeps {
  agents: AgentRepository
  items: ItemRepository
  toolOutputs: ToolOutputRepository
  provider: LLMProvider
  tools: ToolExecutor
  events: EventSink
}

export interface RunContext {
  readonly agents: AgentRepository
  readonly items: ItemRepository
  readonly toolOutputs: ToolOutputRepository
  readonly provider: LLMProvider
  readonly tools: ToolExecutor
  readonly events: EventSink
  agent: Agent
  turnNumber: number
  signal: AbortSignal
  stream: boolean
  /** Awaited for each text delta — ensures each chunk is flushed to the SSE stream before the next. */
  onTextDelta?: (text: string) => Promise<void> | void
}

export interface RunOptions {
  signal?: AbortSignal
  maxTurns?: number
  stream?: boolean
  /** Awaited for each text delta — ensures each chunk is flushed to the SSE stream before the next. */
  onTextDelta?: (text: string) => Promise<void> | void
}

export interface RunResult {
  agentId: string
  status: string
  result?: string
  error?: string
  waitingFor?: WaitingFor[]
  turnCount: number
}
