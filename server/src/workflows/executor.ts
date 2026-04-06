import type { WorkflowDefinition, WorkflowRun } from './types.js'
import type { WorkflowRunRepository } from '../repositories/types.js'
import type { EventSink } from '../events/types.js'
import type { ToolExecutor } from '../tools/types.js'
import type { ProviderRegistry } from '../providers/types.js'
import type { AgentRepository, ItemRepository, ToolOutputRepository, PreferenceRepository } from '../repositories/types.js'
import type { AgentDefinitionRegistry } from '../agents/registry.js'
import type { InterceptHandler } from '../orchestrator/types.js'
import { EVENT_TYPES } from '../events/types.js'
import { buildWorkflowContext } from './context.js'
import { startRun, completeRun, failRun, cancelRun } from './domain.js'
import { logger } from '../lib/logger.js'

export interface ExecutorDeps {
  workflowRuns: WorkflowRunRepository
  events: EventSink
  tools: ToolExecutor
  providers: ProviderRegistry
  agents: AgentRepository
  items: ItemRepository
  toolOutputs: ToolOutputRepository
  preferences: PreferenceRepository
  agentDefinitions: AgentDefinitionRegistry
  interceptHandlers: Map<string, InterceptHandler>
  defaultModel: string
}

/**
 * Resolves as soon as the workflow either completes or calls ctx.discuss().
 * The intercept handler awaits this to decide what to tell the agent.
 */
export type WorkflowReadyResult =
  | { kind: 'completed'; run: WorkflowRun }
  | { kind: 'awaiting_input' }

interface PendingDiscussion {
  resolver: { resolve: (decision: string) => void; reject: (err: Error) => void }
  /** Full workflow execution promise — resolves after discuss() + conclude + remaining steps. */
  execution: Promise<WorkflowRun>
  sessionId: string
}

export class WorkflowExecutor {
  private abortControllers = new Map<string, AbortController>()
  private runningExecutions = new Map<string, Promise<WorkflowRun>>()
  private pendingDiscussions = new Map<string, PendingDiscussion>()
  private sessionToRunId = new Map<string, string>()

  constructor(private deps: ExecutorDeps) {}

  /**
   * Resolve a parked ctx.discuss() call. Called by the globally-registered `conclude` tool.
   * Returns the full execution promise so the tool can await the workflow's final output.
   */
  resolveDiscussionBySession(sessionId: string, decision: string): Promise<WorkflowRun> | null {
    const runId = this.sessionToRunId.get(sessionId)
    if (!runId) return null
    const disc = this.pendingDiscussions.get(runId)
    if (!disc) return null

    this.pendingDiscussions.delete(runId)
    this.sessionToRunId.delete(sessionId)
    disc.resolver.resolve(decision)

    return disc.execution
  }

  /**
   * Start a workflow run.
   *
   * - `execution`: resolves when the workflow fully completes (including after discuss())
   * - `ready`: resolves as soon as the workflow completes OR calls ctx.discuss()
   */
  async start(
    definition: WorkflowDefinition,
    validatedInput: unknown,
    opts: {
      sessionId: string
      signal?: AbortSignal
      triggerAgentId?: string
      triggerCallId?: string
    },
  ): Promise<{ run: WorkflowRun; execution: Promise<WorkflowRun>; ready: Promise<WorkflowReadyResult> }> {
    let run = await this.deps.workflowRuns.create({
      workflowName: definition.name,
      sessionId: opts.sessionId,
      triggerAgentId: opts.triggerAgentId ?? null,
      triggerCallId: opts.triggerCallId ?? null,
      input: validatedInput,
    })

    const ac = new AbortController()
    this.abortControllers.set(run.id, ac)
    const signal = opts.signal
      ? AbortSignal.any([opts.signal, ac.signal])
      : ac.signal

    run = startRun(run)
    run = await this.deps.workflowRuns.update(run.id, {
      status: 'running',
      startedAt: run.startedAt,
    })

    this.deps.events.emit({
      type: EVENT_TYPES.WORKFLOW_STARTED,
      agent_id: run.id,
      session_id: opts.sessionId,
      payload: { runId: run.id, workflowName: definition.name, input: validatedInput },
      timestamp: Date.now(),
    })

    let resolveReady!: (result: WorkflowReadyResult) => void
    const ready = new Promise<WorkflowReadyResult>((r) => { resolveReady = r })
    let readyFired = false
    const fireReady = (result: WorkflowReadyResult) => {
      if (!readyFired) {
        readyFired = true
        resolveReady(result)
      }
    }

    // Set before execute() so registerDiscussionResolver can safely read it
    // even if ctx.discuss() is called before the first await in definition.run().
    let resolveExecution!: (run: WorkflowRun) => void
    let rejectExecution!: (err: Error) => void
    const executionHandle = new Promise<WorkflowRun>((res, rej) => {
      resolveExecution = res
      rejectExecution = rej
    })
    this.runningExecutions.set(run.id, executionHandle)

    const execution = this.execute(run, definition, validatedInput, signal, opts.sessionId, fireReady)
    execution.then(resolveExecution, rejectExecution)

    return { run, execution: executionHandle, ready }
  }

  async cancel(runId: string): Promise<void> {
    const disc = this.pendingDiscussions.get(runId)
    if (disc) {
      this.pendingDiscussions.delete(runId)
      this.sessionToRunId.delete(disc.sessionId)
      disc.resolver.reject(new Error('Workflow cancelled'))
    }

    const ac = this.abortControllers.get(runId)
    if (ac) {
      ac.abort()
      this.abortControllers.delete(runId)
    }

    const run = await this.deps.workflowRuns.getById(runId)
    if (run && (run.status === 'pending' || run.status === 'running' || run.status === 'awaiting_input')) {
      const cancelled = cancelRun(run)
      await this.deps.workflowRuns.update(runId, {
        status: 'cancelled',
        completedAt: cancelled.completedAt,
      })
    }
  }

  abortAll(): void {
    for (const ac of this.abortControllers.values()) ac.abort()
    this.abortControllers.clear()
    for (const disc of this.pendingDiscussions.values()) {
      disc.resolver.reject(new Error('Server shutting down'))
    }
    this.pendingDiscussions.clear()
    this.sessionToRunId.clear()
  }

  private async execute(
    run: WorkflowRun,
    definition: WorkflowDefinition,
    validatedInput: unknown,
    signal: AbortSignal,
    sessionId: string,
    fireReady: (result: WorkflowReadyResult) => void,
  ): Promise<WorkflowRun> {
    try {
      const ctx = buildWorkflowContext({
        runId: run.id,
        workflowName: definition.name,
        sessionId,
        input: validatedInput,
        signal,
        tools: this.deps.tools,
        events: this.deps.events,
        providers: this.deps.providers,
        agents: this.deps.agents,
        items: this.deps.items,
        toolOutputs: this.deps.toolOutputs,
        preferences: this.deps.preferences,
        workflowRuns: this.deps.workflowRuns,
        agentDefinitions: this.deps.agentDefinitions,
        interceptHandlers: this.deps.interceptHandlers,
        defaultModel: this.deps.defaultModel,
        allowedTools: definition.tools,
        triggerAgentId: run.triggerAgentId ?? undefined,
        triggerCallId: run.triggerCallId ?? undefined,
        registerDiscussionResolver: (resolver) => {
          const execution = this.runningExecutions.get(run.id)!
          this.pendingDiscussions.set(run.id, { resolver, execution, sessionId })
          this.sessionToRunId.set(sessionId, run.id)
          // Unblock the intercept handler so the agent loop can resume and chat with the user
          fireReady({ kind: 'awaiting_input' })
        },
      })

      const output = await definition.run(ctx)

      const current = await this.deps.workflowRuns.getById(run.id)
      if (current && current.status !== 'running') {
        fireReady({ kind: 'completed', run: current })
        this.runningExecutions.delete(run.id)
        this.abortControllers.delete(run.id)
        return current
      }

      const completed = completeRun(run, output)
      const updated = await this.deps.workflowRuns.update(run.id, {
        status: 'completed',
        output,
        completedAt: completed.completedAt,
      })

      this.deps.events.emit({
        type: EVENT_TYPES.WORKFLOW_COMPLETED,
        agent_id: run.id,
        session_id: sessionId,
        payload: { runId: run.id, workflowName: definition.name, output },
        timestamp: Date.now(),
      })

      fireReady({ kind: 'completed', run: updated })
      this.runningExecutions.delete(run.id)
      this.abortControllers.delete(run.id)
      return updated
    } catch (err) {
      const isAbort = (err instanceof DOMException && err.name === 'AbortError')
        || (err instanceof Error && err.message === 'Workflow cancelled')
      const errorMsg = err instanceof Error ? err.message : String(err)

      if (isAbort) {
        const cancelled = cancelRun(run)
        const updated = await this.deps.workflowRuns.update(run.id, {
          status: 'cancelled',
          completedAt: cancelled.completedAt,
        })
        fireReady({ kind: 'completed', run: updated })
        this.runningExecutions.delete(run.id)
        this.abortControllers.delete(run.id)
        return updated
      }

      logger.error({ err, runId: run.id, workflow: definition.name }, 'Workflow run failed')

      const failed = failRun(run, errorMsg)
      const updated = await this.deps.workflowRuns.update(run.id, {
        status: 'failed',
        error: errorMsg,
        completedAt: failed.completedAt,
      })

      this.deps.events.emit({
        type: EVENT_TYPES.WORKFLOW_FAILED,
        agent_id: run.id,
        session_id: sessionId,
        payload: { runId: run.id, workflowName: definition.name, error: errorMsg },
        timestamp: Date.now(),
      })

      fireReady({ kind: 'completed', run: updated })
      this.runningExecutions.delete(run.id)
      this.abortControllers.delete(run.id)
      return updated
    }
  }
}
