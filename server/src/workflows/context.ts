import type { WorkflowContext, WorkflowStep } from './types.js'
import type { ToolExecutor } from '../tools/types.js'
import type { ProviderRegistry } from '../providers/types.js'
import type { EventSink } from '../events/types.js'
import type { AgentRepository, ItemRepository, ToolOutputRepository, PreferenceRepository, WorkflowRunRepository } from '../repositories/types.js'
import type { AgentDefinitionRegistry } from '../agents/registry.js'
import type { InterceptHandler, OrchestratorDeps } from '../orchestrator/types.js'
import { EVENT_TYPES } from '../events/types.js'
import { runAgent } from '../orchestrator/runner.js'
import { splitModelId } from '../lib/model.js'
import { randomUUID } from 'node:crypto'

export interface ContextDeps {
  runId: string
  workflowName: string
  sessionId: string
  input: unknown
  signal: AbortSignal
  tools: ToolExecutor
  events: EventSink
  providers: ProviderRegistry
  agents: AgentRepository
  items: ItemRepository
  toolOutputs: ToolOutputRepository
  preferences: PreferenceRepository
  workflowRuns: WorkflowRunRepository
  agentDefinitions: AgentDefinitionRegistry
  interceptHandlers: Map<string, InterceptHandler>
  sessionFilesRoot: string
  inlineOutputLimitBytes?: number
  defaultModel: string
  /** If set, only these tools can be called via ctx.tool(). */
  allowedTools?: string[]
  /** The agent that called workflow.run — events emitted on this agent's SSE stream. */
  triggerAgentId?: string
  /** The workflow.run tool call ID — links steps to their parent. */
  triggerCallId?: string
  /** Turn number on the trigger agent — used for item ordering. */
  turnNumber?: number
  /** Called by ctx.discuss() to park the workflow and signal the intercept handler to unblock the agent. */
  registerDiscussionResolver?: (resolver: { resolve: (decision: string) => void; reject: (err: Error) => void }) => void
}

export function buildWorkflowContext<TInput>(deps: ContextDeps): WorkflowContext<TInput> {
  return {
    runId: deps.runId,
    sessionId: deps.sessionId,
    input: deps.input as TInput,
    signal: deps.signal,

    async tool(name: string, args: Record<string, unknown>): Promise<unknown> {
      // Block orchestrator-intercept tools (use ctx.agent() instead)
      const meta = deps.tools.getMetadata(name)
      if (meta?.orchestrator_intercept) {
        throw new Error(`Tool ${name} is orchestrator-only — use ctx.agent() for delegation`)
      }

      // Enforce allowed tools if the workflow declared them
      if (deps.allowedTools && !deps.allowedTools.includes(name)) {
        throw new Error(`Tool ${name} is not in this workflow's allowed tools: ${deps.allowedTools.join(', ')}`)
      }

      const result = await deps.tools.execute(name, args, {
        agent_id: deps.runId, // use runId as agent_id for scoping
        session_id: deps.sessionId,
        signal: deps.signal,
      })
      if (!result.ok) {
        throw new Error(`Tool ${name} failed: ${result.error ?? 'unknown error'}`)
      }
      return result.output
    },

    async llm(opts: { prompt: string; schema?: Record<string, unknown>; model?: string }): Promise<unknown> {
      const model = opts.model ?? deps.defaultModel
      const provider = deps.providers.resolve(model)
      const { model: modelName } = splitModelId(model)

      const response = await provider.generate({
        model: modelName,
        messages: [{ role: 'user', content: opts.prompt }],
        structured_output: opts.schema,
        signal: deps.signal,
      })

      return response.content
    },

    async agent(name: string, opts: { task: string }): Promise<string> {
      const agentDef = deps.agentDefinitions.get(name)
      const model = agentDef?.model ?? deps.defaultModel
      const { provider: providerName } = splitModelId(model)

      // Create a real agent record
      const agent = await deps.agents.create({
        sessionId: deps.sessionId,
        task: opts.task,
        config: {
          model,
          provider: providerName,
          max_turns: agentDef?.max_turns ?? 10,
          ...(agentDef?.max_output_tokens ? { max_output_tokens: agentDef.max_output_tokens } : {}),
          max_tool_calls_per_step: 5,
          tool_execution_timeout_ms: 60_000,
          ...(agentDef?.system_prompt ? { system_prompt: agentDef.system_prompt } : {}),
          ...(agentDef?.tools ? { allowed_tools: agentDef.tools } : {}),
        },
      })

      // Seed with task
      await deps.items.create({
        agentId: agent.id,
        type: 'message',
        role: 'user',
        content: opts.task,
        turnNumber: 0,
      })

      // Build orchestrator deps
      const orchestratorDeps: OrchestratorDeps = {
        agents: deps.agents,
        items: deps.items,
        toolOutputs: deps.toolOutputs,
        preferences: deps.preferences,
        provider: deps.providers.resolve(model),
        providers: deps.providers,
        tools: deps.tools,
        events: deps.events,
        agentDefinitions: deps.agentDefinitions,
        sessionFilesRoot: deps.sessionFilesRoot,
        inlineOutputLimitBytes: deps.inlineOutputLimitBytes,
        interceptHandlers: deps.interceptHandlers,
      }

      const result = await runAgent(agent.id, orchestratorDeps, {
        signal: deps.signal,
        stream: true,
      })

      if (result.status === 'completed') {
        return result.result ?? '(completed with no output)'
      }

      throw new Error(`Agent ${name} ended with status: ${result.status} — ${result.error ?? 'unknown'}`)
    },

    async map<T, R>(
      items: T[],
      fn: (item: T, index: number) => Promise<R>,
      opts?: { concurrency?: number },
    ): Promise<R[]> {
      if (items.length === 0) return []
      const concurrency = opts?.concurrency ?? 1
      const results: R[] = new Array(items.length)
      let i = 0

      const workers = Array.from(
        { length: Math.min(concurrency, items.length) },
        async () => {
          while (i < items.length) {
            if (deps.signal.aborted) throw new Error('Workflow cancelled')
            const idx = i++
            results[idx] = await fn(items[idx], idx)
          }
        },
      )
      await Promise.all(workers)
      return results
    },

    async step<R>(name: string, fn: () => Promise<R>): Promise<R> {
      const callId = randomUUID()
      const startedAt = Date.now()

      // Emit start event so the step appears immediately in the UI
      deps.events.emit({
        type: EVENT_TYPES.WORKFLOW_PROGRESS,
        agent_id: deps.runId,
        session_id: deps.sessionId,
        payload: { runId: deps.runId, workflowName: deps.workflowName, event: name, data: { phase: name, status: 'running' } },
        timestamp: startedAt,
      })

      const step: WorkflowStep = { callId, name, startedAt }

      try {
        const result = await fn()

        step.completedAt = Date.now()
        step.output = result

        // Emit completion event with output
        deps.events.emit({
          type: EVENT_TYPES.WORKFLOW_PROGRESS,
          agent_id: deps.runId,
          session_id: deps.sessionId,
          payload: {
            runId: deps.runId,
            workflowName: deps.workflowName,
            event: name,
            data: { phase: name, status: 'done', output: result, durationMs: step.completedAt - startedAt },
          },
          timestamp: step.completedAt,
        })

        // Persist step to WorkflowRun.steps
        const run = await deps.workflowRuns.getById(deps.runId)
        if (run) {
          const steps = [...run.steps, step]
          await deps.workflowRuns.update(deps.runId, { steps })
        }

        return result
      } catch (err) {
        step.completedAt = Date.now()
        step.error = err instanceof Error ? err.message : String(err)

        // Emit failure event
        deps.events.emit({
          type: EVENT_TYPES.WORKFLOW_PROGRESS,
          agent_id: deps.runId,
          session_id: deps.sessionId,
          payload: {
            runId: deps.runId,
            workflowName: deps.workflowName,
            event: name,
            data: { phase: name, status: 'failed', error: step.error, durationMs: step.completedAt - startedAt },
          },
          timestamp: step.completedAt,
        })

        // Persist failed step
        const run = await deps.workflowRuns.getById(deps.runId)
        if (run) {
          const steps = [...run.steps, step]
          await deps.workflowRuns.update(deps.runId, { steps })
        }

        throw err
      }
    },

    emit(event: string, data: unknown): void {
      deps.events.emit({
        type: EVENT_TYPES.WORKFLOW_PROGRESS,
        agent_id: deps.runId,
        session_id: deps.sessionId,
        payload: { runId: deps.runId, workflowName: deps.workflowName, event, data },
        timestamp: Date.now(),
      })
    },

    async discuss(prompt: string): Promise<string> {
      if (!deps.registerDiscussionResolver) {
        throw new Error('ctx.discuss() is not available in this execution context')
      }

      // Show the workflow's question in the chat — the agent will discuss with the user
      // and call the globally-registered `conclude` tool when ready to proceed.
      deps.events.emit({
        type: EVENT_TYPES.WORKFLOW_DISCUSSION_STARTED,
        agent_id: deps.runId,
        session_id: deps.sessionId,
        payload: { runId: deps.runId, workflowName: deps.workflowName, prompt, timestamp_ms: Date.now() },
        timestamp: Date.now(),
      })

      await deps.workflowRuns.update(deps.runId, { status: 'awaiting_input' })

      // Park until the `conclude` tool resolves this promise
      const decision = await new Promise<string>((resolve, reject) => {
        deps.registerDiscussionResolver!({ resolve, reject })
      })

      await deps.workflowRuns.update(deps.runId, { status: 'running' })
      return decision
    },
  }
}
