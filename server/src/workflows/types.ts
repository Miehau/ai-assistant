import type { z } from 'zod'

// ---------------------------------------------------------------------------
// Workflow run — persisted entity
// ---------------------------------------------------------------------------

export type WorkflowRunStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'

export interface WorkflowStep {
  callId: string
  name: string
  startedAt: number
  completedAt?: number
  output?: unknown
  error?: string
}

export interface WorkflowRun {
  id: string
  workflowName: string
  sessionId: string
  triggerAgentId: string | null
  triggerCallId: string | null
  status: WorkflowRunStatus
  input: unknown
  output: unknown | null
  steps: WorkflowStep[]
  error: string | null
  startedAt: number | null
  completedAt: number | null
  createdAt: number
  updatedAt: number
}

// ---------------------------------------------------------------------------
// Workflow definition — what the user authors
// ---------------------------------------------------------------------------

export interface WorkflowDefinition<TInput = unknown, TOutput = unknown> {
  name: string
  description: string
  inputSchema: z.ZodType<TInput>
  /** If set, only these tools can be called via ctx.tool(). Omit for unrestricted access. */
  tools?: string[]
  /** Set to false to skip loading this workflow on startup. Defaults to true. */
  enabled?: boolean
  run(ctx: WorkflowContext<TInput>): Promise<TOutput>
}

// ---------------------------------------------------------------------------
// Workflow context — passed to user code at runtime
// ---------------------------------------------------------------------------

export interface WorkflowContext<TInput = unknown> {
  readonly runId: string
  readonly sessionId: string
  readonly input: TInput
  readonly signal: AbortSignal

  /** Call a registered tool directly. Throws on failure. */
  tool(name: string, args: Record<string, unknown>): Promise<unknown>

  /** Single LLM completion (not an agent loop). */
  llm(opts: {
    prompt: string
    schema?: Record<string, unknown>
    model?: string
  }): Promise<unknown>

  /** Spawn a full agent and await its result. */
  agent(name: string, opts: { task: string }): Promise<string>

  /** Parallel map with concurrency control. */
  map<T, R>(
    items: T[],
    fn: (item: T, index: number) => Promise<R>,
    opts?: { concurrency?: number },
  ): Promise<R[]>

  /** Execute a named step — persisted as a tool call, output streamed via SSE. */
  step<R>(name: string, fn: () => Promise<R>): Promise<R>

  /** Emit a progress event visible on the SSE stream (ephemeral, not persisted). */
  emit(event: string, data: unknown): void
}

// ---------------------------------------------------------------------------
// Workflow registry — stores definitions
// ---------------------------------------------------------------------------

export interface WorkflowRegistry {
  register<I, O>(def: WorkflowDefinition<I, O>): void
  get(name: string): WorkflowDefinition | undefined
  list(): WorkflowDefinition[]
}
