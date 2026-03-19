export type CallId = string
export type AgentStatus = 'pending' | 'running' | 'waiting' | 'completed' | 'failed' | 'cancelled'
export type WaitType = 'tool' | 'approval' | 'agent' | 'human'
export type ItemType = 'message' | 'function_call' | 'function_call_output' | 'reasoning'
export type ItemRole = 'system' | 'user' | 'assistant'
export type SessionStatus = 'active' | 'archived'

export interface WaitingFor {
  callId: CallId
  type: WaitType
  name: string
  args?: Record<string, unknown>
  description?: string
}

export interface AgentConfig {
  model: string
  provider: string
  max_turns: number
  max_tool_calls_per_step: number
  tool_execution_timeout_ms: number
  system_prompt?: string
  /** If set, only these tool names are visible to the LLM. Orchestrator-intercept tools (e.g. delegate) are always included. */
  allowed_tools?: string[]
}

export interface Agent {
  id: string
  sessionId: string
  parentId: string | null
  sourceCallId: string | null
  depth: number
  task: string
  config: AgentConfig
  status: AgentStatus
  waitingFor: WaitingFor[]
  result: string | null
  error: string | null
  turnCount: number
  plan: Plan | null
  createdAt: number
  updatedAt: number
  completedAt: number | null
}

export interface Session {
  id: string
  userId: string
  rootAgentId: string | null
  title: string | null
  summary: string | null
  status: SessionStatus
  createdAt: number
  updatedAt: number
}

export interface User {
  id: string
  email: string | null
  apiKeyHash: string
  createdAt: number
  updatedAt: number
}

export interface ItemContentBlock {
  type: 'text' | 'image'
  text?: string
  media_type?: string
  data?: string
}

export interface Item {
  id: string
  agentId: string
  sequence: number
  type: ItemType
  role: ItemRole | null
  content: string | null
  callId: string | null
  name: string | null
  arguments: string | null
  output: string | null
  contentBlocks: ItemContentBlock[] | null
  isError: boolean | null
  saveOutput: boolean | null
  turnNumber: number
  durationMs: number | null
  createdAt: number
}

export interface Plan {
  goal: string
  steps: PlanStep[]
}

export interface PlanStep {
  id: string
  description: string
  status: 'pending' | 'running' | 'completed' | 'failed' | 'skipped'
  action?: string
  result?: string
}

export interface ToolOutput {
  id: string
  agentId: string
  callId: string
  toolName: string
  data: unknown
  createdAt: number
}
