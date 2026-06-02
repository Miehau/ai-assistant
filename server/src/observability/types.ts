import type { Agent } from '../domain/types.js'
import type { LLMRequest, LLMResponse } from '../providers/types.js'

export interface AgentTraceContext {
  agent: Agent
  input?: unknown
  inputSource?: string
}

export interface GenerationTraceContext {
  agent: Agent
  turnNumber: number
  provider: string
  model: string
  request: LLMRequest
  stream: boolean
}

export interface ToolTraceContext {
  agent: Agent
  callId: string
  name: string
  args: Record<string, unknown>
  approved?: boolean
}

export interface LLMObservability {
  readonly enabled: boolean
  traceAgent<T extends { status: string; error?: string; result?: string; turnCount?: number }>(
    context: AgentTraceContext,
    fn: () => Promise<T>,
  ): Promise<T>
  traceGeneration<T extends LLMResponse>(
    context: GenerationTraceContext,
    fn: () => Promise<T>,
  ): Promise<T>
  traceTool<T>(
    context: ToolTraceContext,
    fn: () => Promise<T>,
  ): Promise<T>
  shutdown(): Promise<void>
}
