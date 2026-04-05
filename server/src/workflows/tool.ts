import type { ToolHandler } from '../tools/types.js'
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

    // Execute workflow synchronously — await completion so the agent loop
    // (and its SSE stream) stays alive throughout the workflow run.
    const { execution } = await executor.start(definition, parsed.data, {
      sessionId: ctx.agent.sessionId,
      signal: ctx.signal,
      triggerAgentId: ctx.agent.id,
      triggerCallId: callId,
    })

    let output: string
    let isError: boolean

    try {
      const completedRun = await execution
      isError = completedRun.status !== 'completed'
      output = isError
        ? `Workflow ${completedRun.status}: ${completedRun.error ?? 'unknown'}`
        : JSON.stringify(completedRun.output ?? '(workflow completed with no output)')
    } catch (err) {
      isError = true
      output = `Workflow error: ${err instanceof Error ? err.message : String(err)}`
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
