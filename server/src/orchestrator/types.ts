import type { Agent, WaitingFor } from '../domain/types.js'
import type { LLMProvider, ProviderRegistry } from '../providers/types.js'
import type { ToolExecutor } from '../tools/types.js'
import type { AgentRepository, ItemRepository, ToolOutputRepository, PreferenceRepository } from '../repositories/types.js'
import type { EventSink } from '../events/types.js'
import type { AgentDefinitionRegistry } from '../agents/registry.js'

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

/**
 * Intercept handler — a plugin function that the runner calls instead of
 * the tool registry for tools marked `orchestrator_intercept: true`.
 *
 * Registered by name in `OrchestratorDeps.interceptHandlers`.
 * The runner dispatches to the matching handler; it has no knowledge of
 * what the handler does (delegation, workflow, etc.).
 */
export type InterceptHandler = (
  callId: string,
  args: Record<string, unknown>,
  ctx: RunContext,
  deps: OrchestratorDeps,
  startMs: number,
) => Promise<StepExecutionOutcome>

export interface OrchestratorDeps {
  agents: AgentRepository
  items: ItemRepository
  toolOutputs: ToolOutputRepository
  preferences: PreferenceRepository
  provider: LLMProvider
  /** Provider registry for resolving child agent providers during delegation */
  providers?: ProviderRegistry
  tools: ToolExecutor
  events: EventSink
  agentDefinitions: AgentDefinitionRegistry
  /** Managed root for session-scoped workspace/artifact files. */
  sessionFilesRoot: string
  /** Inline output limit before output is replaced with an artifact reference. */
  inlineOutputLimitBytes?: number
  /** Pluggable intercept handlers — keyed by tool name (e.g. 'delegate', 'workflow.run'). */
  interceptHandlers?: Map<string, InterceptHandler>
}

export interface RunContext {
  readonly agents: AgentRepository
  readonly items: ItemRepository
  readonly toolOutputs: ToolOutputRepository
  readonly preferences: PreferenceRepository
  readonly provider: LLMProvider
  readonly providers?: ProviderRegistry
  readonly tools: ToolExecutor
  readonly events: EventSink
  readonly agentDefinitions: AgentDefinitionRegistry
  readonly sessionFilesRoot: string
  readonly inlineOutputLimitBytes?: number
  readonly interceptHandlers?: Map<string, InterceptHandler>
  agent: Agent
  turnNumber: number
  signal: AbortSignal
  stream: boolean
}

export interface RunOptions {
  signal?: AbortSignal
  maxTurns?: number
  stream?: boolean
}

export interface RunResult {
  agentId: string
  status: string
  result?: string
  error?: string
  waitingFor?: WaitingFor[]
  turnCount: number
}
