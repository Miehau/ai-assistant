import type { ToolHandler, ToolContext } from '../tools/types.js'
import type { InterceptHandler, RunContext, OrchestratorDeps, StepExecutionOutcome } from '../orchestrator/types.js'
import type { WorkflowRegistry, WorkflowDefinition } from './types.js'
import type { WorkflowExecutor } from './executor.js'
import { EVENT_TYPES } from '../events/types.js'

/**
 * Registers the `workflow.run` tool and its intercept handler.
 *
 * The tool is marked `orchestrator_intercept: true` — the runner dispatches
 * to the registered InterceptHandler instead of calling handle().
 *
 * Execution is **synchronous** from the agent's perspective: the intercept
 * handler awaits the workflow, stores the result as a function_call_output,
 * and returns `{ type: 'continue' }` so the controller loop keeps running.
 * This keeps the SSE stream open throughout the workflow so progress events
 * reach the client.
 */
export function registerWorkflowTools(
  registry: { register: (h: ToolHandler) => void },
  workflowRegistry: WorkflowRegistry,
  executor: WorkflowExecutor,
  interceptHandlers?: Map<string, InterceptHandler>,
): void {
  const workflows = workflowRegistry.list()
  if (workflows.length === 0) return // no workflows, no tool

  const names = workflows.map((w) => w.name)
  const description = buildWorkflowToolDescription(workflows)

  registry.register({
    metadata: {
      name: 'workflow.run',
      description,
      parameters: {
        type: 'object',
        properties: {
          name: {
            type: 'string',
            enum: names,
            description: `Workflow to run. Available: ${names.join(', ')}`,
          },
          input: {
            type: 'object',
            description: 'Input data for the workflow (validated against its schema). See per-workflow input requirements in the tool description.',
          },
        },
        required: ['name', 'input'],
      },
      requires_approval: false,
      orchestrator_intercept: true,
    },
    async handle() {
      return { ok: false, error: 'workflow.run must be intercepted by the orchestrator — this is a bug' }
    },
  })

  // Register the `conclude` tool so the LLM can resume a parked ctx.discuss()
  registry.register({
    metadata: {
      name: 'conclude',
      description:
        'Resume a workflow that is awaiting your decision after ctx.discuss(). ' +
        'Call this once you have discussed with the user and are ready to proceed.',
      parameters: {
        type: 'object',
        properties: {
          decision: {
            type: 'string',
            description: 'Your decision or summary to pass back to the workflow.',
          },
        },
        required: ['decision'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext) {
      const decision = args.decision as string
      if (!decision) {
        return { ok: false as const, error: 'conclude requires "decision"' }
      }

      const execution = executor.resolveDiscussionBySession(ctx.session_id, decision)
      if (!execution) {
        return { ok: false as const, error: 'No workflow is currently awaiting a decision in this session. Do not retry — only call conclude when a workflow has paused for discussion.' }
      }

      try {
        const completedRun = await execution
        const output = completedRun.status !== 'completed'
          ? `Workflow ${completedRun.status}: ${completedRun.error ?? 'unknown'}`
          : JSON.stringify(completedRun.output ?? '(workflow completed with no output)')
        return { ok: true as const, output }
      } catch (err) {
        return { ok: false as const, error: err instanceof Error ? err.message : String(err) }
      }
    },
  })

  // Self-register the intercept handler
  interceptHandlers?.set('workflow.run', createWorkflowInterceptHandler(workflowRegistry, executor))
}

// ---------------------------------------------------------------------------
// Build a rich tool description that includes per-workflow input docs
// ---------------------------------------------------------------------------

function describeZodShape(schema: import('zod').ZodType): string {
  // Walk the Zod schema to produce a readable input spec.
  // We handle the common cases (ZodObject, ZodDefault) — anything exotic
  // falls back to a generic "(see workflow definition)" hint.
  const inner = unwrapDefault(schema)
  if (!inner || !('shape' in inner)) return '  (no input required)'

  const shape = (inner as import('zod').ZodObject<any>).shape as Record<string, import('zod').ZodType>
  const lines: string[] = []
  for (const [key, field] of Object.entries(shape)) {
    const desc = field.description ?? ''
    const opt = field.isOptional() ? ' (optional)' : ''
    lines.push(`  - ${key}${opt}: ${desc || inferZodTypeName(field)}`)
  }
  return lines.length > 0 ? lines.join('\n') : '  (no input required)'
}

function unwrapDefault(schema: import('zod').ZodType): import('zod').ZodType {
  // ZodDefault wraps an inner schema — unwrap it so we can inspect shape
  if ('_def' in schema && (schema as any)._def?.innerType) {
    return (schema as any)._def.innerType
  }
  return schema
}

function inferZodTypeName(field: import('zod').ZodType): string {
  const typeName = (field as any)._def?.typeName as string | undefined
  if (!typeName) return 'unknown'
  return typeName.replace('Zod', '').toLowerCase()
}

function buildWorkflowToolDescription(workflows: WorkflowDefinition[]): string {
  const header =
    `Run a registered workflow pipeline. Workflows execute procedural steps ` +
    `(tool calls, LLM classifications, file operations) deterministically.`

  const sections = workflows.map((w) => {
    const inputDoc = describeZodShape(w.inputSchema)
    return `### ${w.name}\n${w.description}\nInput:\n${inputDoc}`
  })

  return `${header}\n\nAvailable workflows:\n\n${sections.join('\n\n')}`
}

function createWorkflowInterceptHandler(
  workflowRegistry: WorkflowRegistry,
  executor: WorkflowExecutor,
): InterceptHandler {
  return async (
    callId: string,
    args: Record<string, unknown>,
    ctx: RunContext,
    deps: OrchestratorDeps,
    startMs: number,
  ): Promise<StepExecutionOutcome> => {
    const workflowName = args.name as string
    const rawInput = args.input ?? {}

    if (!workflowName) {
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'function_call_output',
        callId,
        output: 'workflow.run requires "name"',
        isError: true,
        turnNumber: ctx.turnNumber,
      })
      return { type: 'continue' }
    }

    const definition = workflowRegistry.get(workflowName)
    if (!definition) {
      const available = workflowRegistry.list().map((w) => w.name).join(', ') || 'none'
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'function_call_output',
        callId,
        output: `Unknown workflow: "${workflowName}". Available: ${available}`,
        isError: true,
        turnNumber: ctx.turnNumber,
      })
      return { type: 'continue' }
    }

    // Validate input against the workflow's Zod schema
    const parsed = definition.inputSchema.safeParse(rawInput)
    if (!parsed.success) {
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'function_call_output',
        callId,
        output: `Invalid workflow input: ${parsed.error.message}`,
        isError: true,
        turnNumber: ctx.turnNumber,
      })
      return { type: 'continue' }
    }

    // Start the workflow. `ready` resolves as soon as the workflow either
    // completes normally OR parks at ctx.discuss(). `execution` resolves only
    // after the full workflow finishes (including after a discuss/conclude cycle).
    const { execution, ready } = await executor.start(definition, parsed.data, {
      sessionId: ctx.agent.sessionId,
      signal: ctx.signal,
      triggerAgentId: ctx.agent.id,
      triggerCallId: callId,
    })

    // Prevent unhandled-rejection warnings if conclude is never called
    execution.catch(() => {})

    const readyResult = await ready

    let output: string
    let isError: boolean

    if (readyResult.kind === 'awaiting_input') {
      // Workflow parked at ctx.discuss() — unblock the agent so it can chat
      // with the user. The `conclude` tool will resume the workflow.
      output = `Workflow "${workflowName}" has paused for discussion. Chat with the user and call \`conclude\` with your decision when ready to proceed.`
      isError = false
    } else {
      const completedRun = readyResult.run
      isError = completedRun.status !== 'completed'
      output = isError
        ? `Workflow ${completedRun.status}: ${completedRun.error ?? 'unknown'}`
        : JSON.stringify(completedRun.output ?? '(workflow completed with no output)')
    }

    // Emit tool_end so the client closes the tool bubble
    deps.events.emit({
      type: EVENT_TYPES.TOOL_COMPLETED,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: {
        callId,
        name: 'workflow.run',
        success: !isError,
        output,
        durationMs: Date.now() - startMs,
        parentId: ctx.agent.parentId,
        depth: ctx.agent.depth,
      },
      timestamp: Date.now(),
    })

    // Store result so the LLM sees it on the next turn
    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call_output',
      callId,
      output,
      isError,
      turnNumber: ctx.turnNumber,
    })

    return { type: 'continue' }
  }
}
