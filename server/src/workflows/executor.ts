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

export class WorkflowExecutor {
  private abortControllers = new Map<string, AbortController>()

  constructor(private deps: ExecutorDeps) {}

  /**
   * Start a workflow run. Returns the initial WorkflowRun record.
   * The actual execution happens in the returned promise (awaitable or fire-and-forget).
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
  ): Promise<{ run: WorkflowRun; execution: Promise<WorkflowRun> }> {
    // Create the run record
    let run = await this.deps.workflowRuns.create({
      workflowName: definition.name,
      sessionId: opts.sessionId,
      triggerAgentId: opts.triggerAgentId ?? null,
      triggerCallId: opts.triggerCallId ?? null,
      input: validatedInput,
    })

    // Set up abort controller
    const ac = new AbortController()
    this.abortControllers.set(run.id, ac)
    const signal = opts.signal
      ? AbortSignal.any([opts.signal, ac.signal])
      : ac.signal

    // Transition to running
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

    // Build the execution promise
    const execution = this.execute(run, definition, validatedInput, signal, opts.sessionId)

    return { run, execution }
  }

  async cancel(runId: string): Promise<void> {
    const ac = this.abortControllers.get(runId)
    if (ac) {
      ac.abort()
      this.abortControllers.delete(runId)
    }

    const run = await this.deps.workflowRuns.getById(runId)
    if (run && (run.status === 'pending' || run.status === 'running')) {
      const cancelled = cancelRun(run)
      await this.deps.workflowRuns.update(runId, {
        status: 'cancelled',
        completedAt: cancelled.completedAt,
      })
    }
  }

  abortAll(): void {
    for (const ac of this.abortControllers.values()) {
      ac.abort()
    }
    this.abortControllers.clear()
  }

  private async execute(
    run: WorkflowRun,
    definition: WorkflowDefinition,
    validatedInput: unknown,
    signal: AbortSignal,
    sessionId: string,
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
      })

      const output = await definition.run(ctx)

      // Re-read from DB to guard against cancel() race
      const current = await this.deps.workflowRuns.getById(run.id)
      if (current && current.status !== 'running') {
        // cancel() already moved this to a terminal state — don't overwrite
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

      this.abortControllers.delete(run.id)
      return updated
    } catch (err) {
      const isAbort = err instanceof DOMException && err.name === 'AbortError'
      const errorMsg = err instanceof Error ? err.message : String(err)

      if (isAbort) {
        const cancelled = cancelRun(run)
        const updated = await this.deps.workflowRuns.update(run.id, {
          status: 'cancelled',
          completedAt: cancelled.completedAt,
        })
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

      this.abortControllers.delete(run.id)
      return updated
    }
  }
}
