export interface ToolExecutor {
  execute(
    name: string,
    args: Record<string, unknown>,
    ctx: ToolContext,
  ): Promise<ToolResult>
  executeBatch(calls: ToolCall[], ctx: ToolContext): Promise<ToolBatchResult>
  getMetadata(name: string): ToolMetadata | undefined
  listMetadata(): ToolMetadata[]
  unregister(name: string): boolean
  validateArgs(name: string, args: unknown): ValidationResult
  getPreview(name: string, args: Record<string, unknown>, ctx: ToolContext): ToolPreview | undefined
}

export interface ToolMetadata {
  name: string
  description: string
  parameters: Record<string, unknown> // JSON Schema
  requires_approval: boolean
  /** Tool is intercepted by the orchestrator — handler is not called directly. */
  orchestrator_intercept?: boolean
}

export interface ToolContext {
  agent_id: string
  session_id: string
  signal: AbortSignal
}

export interface ToolResult {
  ok: boolean
  output?: unknown
  error?: string
  content_blocks?: ContentBlock[]
}

export interface ContentBlock {
  type: 'text' | 'image'
  text?: string
  media_type?: string
  data?: string
}

export interface ToolCall {
  call_id: string
  name: string
  args: Record<string, unknown>
  save?: boolean
}

export interface ToolBatchResult {
  results: Array<{ call_id: string } & ToolResult>
  all_ok: boolean
}

export interface ValidationResult {
  valid: boolean
  errors?: string[]
}

export interface ToolPreview {
  summary: string
  details?: Record<string, unknown>
}

// Internal to tools/ module — not used by orchestrator
export interface ToolHandler {
  metadata: ToolMetadata
  handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult>
  preview?(args: Record<string, unknown>, ctx: ToolContext): ToolPreview
}
