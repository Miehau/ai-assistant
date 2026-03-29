import { randomUUID } from 'node:crypto'
import type {
  ControllerAction,
  OrchestratorDeps,
  RunContext,
  RunOptions,
  RunResult,
  StepExecutionOutcome,
  ToolCallSpec,
} from './types.js'
import type { WaitingFor } from '../domain/types.js'
import type { LLMProvider, LLMRequest, LLMToolDefinition, LLMResponse } from '../providers/types.js'
import type { ToolCall } from '../tools/types.js'
import { startAgent, completeAgent, failAgent, waitForMany } from '../domain/agent.js'
import {
  parseControllerAction,
  mapToolCallsToAction,
  inferStepType,
  controllerOutputSchema,
} from './parsing.js'
import {
  CONTROLLER_PROMPT_BASE,
  CONTROLLER_PROMPT_ANTHROPIC,
  CONTROLLER_PROMPT_OPENAI,
  buildControllerMessages,
  buildToolListString,
} from './prompts.js'
import { formatToolOutput } from './output.js'
import { hydrateToolArgs } from './hydration.js'
import { EVENT_TYPES } from '../events/types.js'

const DEFAULT_MAX_TURNS = 50
const MAX_AGENT_DEPTH = 5

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

export async function runAgent(
  agentId: string,
  deps: OrchestratorDeps,
  options?: RunOptions,
): Promise<RunResult> {
  // Load agent
  let agent = await deps.agents.getById(agentId)
  if (!agent) throw new Error(`Agent not found: ${agentId}`)

  const maxTurns = options?.maxTurns ?? agent.config.max_turns ?? DEFAULT_MAX_TURNS
  const signal = options?.signal ?? AbortSignal.timeout(10 * 60 * 1000) // 10 min default

  // Transition pending → running
  if (agent.status === 'pending') {
    agent = startAgent(agent)
    agent = await deps.agents.update(agentId, { status: 'running' })
  }

  if (agent.status !== 'running') {
    return {
      agentId,
      status: agent.status,
      result: agent.result ?? undefined,
      error: agent.error ?? undefined,
      waitingFor: agent.waitingFor.length > 0 ? agent.waitingFor : undefined,
      turnCount: agent.turnCount,
    }
  }

  deps.events.emit({
    type: EVENT_TYPES.AGENT_STARTED,
    agent_id: agentId,
    session_id: agent.sessionId,
    payload: { task: agent.task, model: agent.config.model, parentId: agent.parentId, depth: agent.depth },
    timestamp: Date.now(),
  })

  const ctx: RunContext = {
    agents: deps.agents,
    items: deps.items,
    toolOutputs: deps.toolOutputs,
    preferences: deps.preferences,
    provider: deps.provider,
    tools: deps.tools,
    events: deps.events,
    agentDefinitions: deps.agentDefinitions,
    agent,
    turnNumber: 0,
    signal,
    stream: options?.stream ?? false,
  }

  try {
    // Resume: execute any approved-but-not-yet-executed tools
    await executePendingApprovedTools(ctx)

    // Main controller loop
    while (ctx.agent.status === 'running' && ctx.turnNumber < maxTurns) {
      if (signal.aborted) {
        throw new Error('Agent run aborted')
      }

      ctx.turnNumber++

      deps.events.emit({
        type: EVENT_TYPES.TURN_STARTED,
        agent_id: agentId,
        session_id: ctx.agent.sessionId,
        payload: { turn: ctx.turnNumber, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
        timestamp: Date.now(),
      })

      // 1. Build messages
      const useNativeTools = isNativeToolProvider(ctx.agent.config.provider)
      const systemPrompt = selectSystemPrompt(ctx.agent.config.provider)
      const allToolMetadata = ctx.tools.listMetadata()
      const allowedTools = ctx.agent.config.allowed_tools
      let toolMetadata = allowedTools
        ? allToolMetadata.filter((t) => t.orchestrator_intercept || allowedTools.includes(t.name))
        : allToolMetadata

      // Subagents should execute work, not re-plan or re-delegate
      if (ctx.agent.depth > 0) {
        toolMetadata = toolMetadata.filter((t) => t.name !== 'delegate' && !t.name.startsWith('tasks.'))
      }
      const toolListStr = buildToolListString(toolMetadata)

      // Load this agent's own items. Root agents already have complete context:
      // user messages, tool call/output pairs, and delegate results (which summarise
      // child agent work). Using listBySession would include child agent items after
      // the root's items, breaking chronological order on follow-up messages and
      // burying the user's latest message mid-history.
      const items = await ctx.items.listByAgent(agentId)
      const messages = buildControllerMessages(systemPrompt, toolListStr, items, {
        useNativeFunctionCalling: useNativeTools,
        agentTask: ctx.agent.task,
        customSystemPrompt: ctx.agent.config.system_prompt,
      })

      // 2. Call LLM provider
      const toolDefs: LLMToolDefinition[] | undefined = useNativeTools
        ? toolMetadata.map((t) => ({
            name: t.name,
            description: t.description,
            parameters: t.parameters,
          }))
        : undefined

      // Strip provider prefix ("anthropic:claude-3" → "claude-3")
      const modelName = ctx.agent.config.model.includes(':')
        ? ctx.agent.config.model.slice(ctx.agent.config.model.indexOf(':') + 1)
        : ctx.agent.config.model

      const llmRequest: LLMRequest = {
        model: modelName,
        messages,
        tools: toolDefs,
        structured_output: !useNativeTools ? controllerOutputSchema() : undefined,
        signal,
      }

      // Only stream for native tool providers — non-native providers output raw JSON
      // which is meaningless to stream character-by-character to the UI.
      const llmResponse: LLMResponse = (ctx.stream && useNativeTools)
        ? await streamLLMTurn(ctx.provider, llmRequest, deps, agentId, ctx.agent.sessionId, ctx.agent.parentId, ctx.agent.depth)
        : await ctx.provider.generate(llmRequest)

      // 3. Handle companion text (thinking/reasoning shown alongside tool calls)
      if (llmResponse.companion_text) {
        // When streaming, text was already sent as deltas — skip the COMPANION_TEXT event
        if (!ctx.stream) {
          deps.events.emit({
            type: EVENT_TYPES.COMPANION_TEXT,
            agent_id: agentId,
            session_id: ctx.agent.sessionId,
            payload: { text: llmResponse.companion_text, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
            timestamp: Date.now(),
          })
        }
        await ctx.items.create({
          agentId,
          type: 'reasoning',
          role: 'assistant',
          content: llmResponse.companion_text,
          turnNumber: ctx.turnNumber,
        })
      }

      // 4. Parse response into ControllerAction
      let action: ControllerAction
      if (llmResponse.tool_calls && llmResponse.tool_calls.length > 0) {
        action = mapToolCallsToAction(llmResponse.tool_calls)
      } else if (useNativeTools) {
        // Native tool providers respond with plain text when they're done — treat as complete
        action = {
          action: 'complete',
          message: typeof llmResponse.content === 'string'
            ? llmResponse.content
            : String(llmResponse.content),
        }
      } else {
        action = parseControllerAction(llmResponse.content)
      }

      deps.events.emit({
        type: EVENT_TYPES.STEP_PROPOSED,
        agent_id: agentId,
        session_id: ctx.agent.sessionId,
        payload: { action: action.action, turn: ctx.turnNumber, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
        timestamp: Date.now(),
      })

      // 5. Execute action
      const outcome = await executeAction(ctx, action, llmResponse)

      deps.events.emit({
        type: EVENT_TYPES.TURN_COMPLETED,
        agent_id: agentId,
        session_id: ctx.agent.sessionId,
        payload: { turn: ctx.turnNumber, outcome: outcome.type, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
        timestamp: Date.now(),
      })

      // 6. Save turn count
      await ctx.agents.update(agentId, { turnCount: ctx.turnNumber })

      // 7. Check outcome
      if (outcome.type === 'complete') {
        const completed = completeAgent(ctx.agent, outcome.response)
        ctx.agent = await ctx.agents.update(agentId, {
          status: 'completed',
          result: completed.result,
          completedAt: completed.completedAt,
        })

        deps.events.emit({
          type: EVENT_TYPES.AGENT_COMPLETED,
          agent_id: agentId,
          session_id: ctx.agent.sessionId,
          payload: { result: outcome.response, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
          timestamp: Date.now(),
        })

        return {
          agentId,
          status: 'completed',
          result: outcome.response,
          turnCount: ctx.turnNumber,
        }
      }

      if (outcome.type === 'waiting') {
        return {
          agentId,
          status: 'waiting',
          waitingFor: outcome.waiting_for,
          turnCount: ctx.turnNumber,
        }
      }

      // outcome.type === 'continue' — loop again
      // Refresh agent state from DB (it may have been updated by tool execution)
      const refreshed = await ctx.agents.getById(agentId)
      if (refreshed) ctx.agent = refreshed
    }

    // Max turns exceeded
    const errorMsg = `Agent reached maximum turn limit (${maxTurns})`
    const failed = failAgent(ctx.agent, errorMsg)
    ctx.agent = await ctx.agents.update(agentId, {
      status: 'failed',
      error: errorMsg,
      completedAt: failed.completedAt,
    })

    deps.events.emit({
      type: EVENT_TYPES.AGENT_FAILED,
      agent_id: agentId,
      session_id: ctx.agent.sessionId,
      payload: { error: errorMsg, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })

    return {
      agentId,
      status: 'failed',
      error: errorMsg,
      turnCount: ctx.turnNumber,
    }
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err)
    const failed = failAgent(ctx.agent, errorMsg)
    await ctx.agents.update(agentId, {
      status: 'failed',
      error: errorMsg,
      completedAt: failed.completedAt,
    })

    deps.events.emit({
      type: EVENT_TYPES.AGENT_FAILED,
      agent_id: agentId,
      session_id: ctx.agent.sessionId,
      payload: { error: errorMsg, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })

    return {
      agentId,
      status: 'failed',
      error: errorMsg,
      turnCount: ctx.turnNumber,
    }
  }
}

// ---------------------------------------------------------------------------
// Streaming LLM turn
// ---------------------------------------------------------------------------

async function streamLLMTurn(
  provider: LLMProvider,
  request: LLMRequest,
  deps: OrchestratorDeps,
  agentId: string,
  sessionId: string,
  parentId?: string | null,
  depth?: number,
): Promise<LLMResponse> {
  const streamIter = provider.stream(request)
  let finalResponse: LLMResponse | null = null

  for await (const event of streamIter) {
    switch (event.type) {
      case 'text_delta':
        // Emit to event bus — the SSE handler consumes TEXT_DELTA events in order
        // with all other events, ensuring correct interleaving without concurrent writes.
        deps.events.emit({
          type: EVENT_TYPES.TEXT_DELTA,
          agent_id: agentId,
          session_id: sessionId,
          payload: { text: event.text, parentId: parentId ?? null, depth: depth ?? 0 },
          timestamp: Date.now(),
        })
        break
      case 'done':
        finalResponse = event.response
        break
      case 'error':
        throw new Error(`LLM stream error: ${event.error}`)
    }
  }

  if (!finalResponse) {
    throw new Error('LLM stream ended without a done event')
  }

  // Fallback: some reasoning models (GPT-5 family) return empty content when
  // streaming via chat completions but work fine non-streaming.  Retry once.
  if (!finalResponse.content && !finalResponse.tool_calls?.length) {
    console.warn('[streamLLMTurn] Streaming returned empty content — falling back to non-streaming generate()')
    return provider.generate(request)
  }

  return finalResponse
}

// ---------------------------------------------------------------------------
// Provider detection
// ---------------------------------------------------------------------------

function isNativeToolProvider(provider: string): boolean {
  const native = ['anthropic', 'openai', 'deepseek', 'ollama', 'openrouter']
  return native.includes(provider.toLowerCase())
}

function selectSystemPrompt(provider: string): string {
  switch (provider.toLowerCase()) {
    case 'anthropic':
      return CONTROLLER_PROMPT_ANTHROPIC
    case 'openai':
    case 'deepseek':
    case 'openrouter':
      return CONTROLLER_PROMPT_OPENAI
    default:
      return CONTROLLER_PROMPT_BASE
  }
}

// ---------------------------------------------------------------------------
// Execute pending approved tools on resume
// ---------------------------------------------------------------------------

async function executePendingApprovedTools(ctx: RunContext): Promise<void> {
  const items = await ctx.items.listByAgent(ctx.agent.id)

  // Find function_call items without a matching function_call_output
  const callItems = items.filter((i) => i.type === 'function_call')
  const outputCallIds = new Set(
    items.filter((i) => i.type === 'function_call_output').map((i) => i.callId),
  )

  const pendingCalls = callItems.filter(
    (i) => i.callId && !outputCallIds.has(i.callId),
  )

  for (const callItem of pendingCalls) {
    const toolName = callItem.name!
    const toolArgs: Record<string, unknown> = callItem.arguments
      ? JSON.parse(callItem.arguments)
      : {}
    const callId = callItem.callId!

    ctx.events.emit({
      type: EVENT_TYPES.TOOL_STARTED,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { callId, name: toolName, args: toolArgs, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })

    const startMs = Date.now()
    const result = await ctx.tools.execute(toolName, toolArgs, {
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      signal: ctx.signal,
    })
    const durationMs = Date.now() - startMs

    const outputStr = formatToolOutput(result)

    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call_output',
      callId,
      output: outputStr,
      contentBlocks: result.content_blocks ?? null,
      isError: !result.ok,
      turnNumber: ctx.turnNumber,
      durationMs,
    })

    // Persist large outputs
    if (result.ok && callItem.saveOutput) {
      await ctx.toolOutputs.save({
        agentId: ctx.agent.id,
        callId,
        toolName,
        data: result.output,
      })
    }

    ctx.events.emit({
      type: EVENT_TYPES.TOOL_COMPLETED,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { callId, name: toolName, success: result.ok, output: outputStr, durationMs, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })
  }
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

async function executeAction(
  ctx: RunContext,
  action: ControllerAction,
  llmResponse: LLMResponse,
): Promise<StepExecutionOutcome> {
  switch (action.action) {
    case 'next_step':
      return executeFlatStep(ctx, action, llmResponse)

    case 'complete':
      // Save the completion message as an assistant item
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'message',
        role: 'assistant',
        content: action.message,
        turnNumber: ctx.turnNumber,
      })
      return { type: 'complete', response: action.message }

    case 'guardrail_stop': {
      const msg = action.message ?? `Stopped: ${action.reason}`
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'message',
        role: 'assistant',
        content: msg,
        turnNumber: ctx.turnNumber,
      })
      const failed = failAgent(ctx.agent, `Guardrail stop: ${action.reason}`)
      ctx.agent = await ctx.agents.update(ctx.agent.id, {
        status: 'failed',
        error: `Guardrail stop: ${action.reason}`,
        completedAt: failed.completedAt,
      })
      return { type: 'complete', response: msg }
    }

    case 'ask_user':
      return executeAskUser(ctx, action.question, action.context)

    default:
      throw new Error(`Unknown action: ${(action as ControllerAction).action}`)
  }
}

// ---------------------------------------------------------------------------
// Flat step execution
// ---------------------------------------------------------------------------

async function executeFlatStep(
  ctx: RunContext,
  action: ControllerAction & { action: 'next_step' },
  llmResponse: LLMResponse,
): Promise<StepExecutionOutcome> {
  const stepType = inferStepType(action)

  ctx.events.emit({
    type: EVENT_TYPES.STEP_STARTED,
    agent_id: ctx.agent.id,
    session_id: ctx.agent.sessionId,
    payload: { stepType: stepType ?? 'unknown', turn: ctx.turnNumber, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
    timestamp: Date.now(),
  })

  let outcome: StepExecutionOutcome

  switch (stepType) {
    case 'tool': {
      // Use LLM tool_calls call_id if available, else generate one
      const callId =
        llmResponse.tool_calls?.[0]?.call_id ?? randomUUID()
      outcome = await executeTool(
        ctx,
        action.tool!,
        action.args ?? {},
        callId,
        action.save,
      )
      break
    }

    case 'tool_batch': {
      // Map call IDs from LLM response if available
      const specs: Array<ToolCallSpec & { callId: string }> = action.tools!.map(
        (t, i) => ({
          ...t,
          callId: llmResponse.tool_calls?.[i]?.call_id ?? randomUUID(),
        }),
      )
      outcome = await executeToolBatch(ctx, specs)
      break
    }

    case 'respond': {
      // Assistant message — save and complete
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'message',
        role: 'assistant',
        content: action.message!,
        turnNumber: ctx.turnNumber,
      })
      outcome = { type: 'complete', response: action.message! }
      break
    }

    case 'ask_user': {
      outcome = await executeAskUser(ctx, action.question!, action.context)
      break
    }

    default:
      throw new Error(`Cannot infer step type from next_step action fields`)
  }

  ctx.events.emit({
    type: EVENT_TYPES.STEP_COMPLETED,
    agent_id: ctx.agent.id,
    session_id: ctx.agent.sessionId,
    payload: { stepType: stepType ?? 'unknown', turn: ctx.turnNumber, outcomeType: outcome.type, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
    timestamp: Date.now(),
  })

  return outcome
}

// ---------------------------------------------------------------------------
// Approval override resolution
// ---------------------------------------------------------------------------

const APPROVAL_PREFIX_SESSION = 'tool_approval_session:'
const APPROVAL_PREFIX_GLOBAL = 'tool_approval_global:'

async function resolveRequiresApproval(
  ctx: RunContext,
  toolName: string,
  defaultRequiresApproval: boolean,
): Promise<boolean> {
  try {
    // 1. Session-scoped override (highest priority)
    const sessionKey = `${APPROVAL_PREFIX_SESSION}${ctx.agent.sessionId}:${toolName}`
    const sessionOverride = await ctx.preferences.get(sessionKey)
    if (sessionOverride !== null) return sessionOverride === 'true'

    // 2. Global override
    const globalKey = `${APPROVAL_PREFIX_GLOBAL}${toolName}`
    const globalOverride = await ctx.preferences.get(globalKey)
    if (globalOverride !== null) return globalOverride === 'true'
  } catch {
    // Fall through to default on any error
  }

  return defaultRequiresApproval
}

// ---------------------------------------------------------------------------
// Single tool execution
// ---------------------------------------------------------------------------

async function executeTool(
  ctx: RunContext,
  name: string,
  args: Record<string, unknown>,
  callId: string,
  save?: boolean,
): Promise<StepExecutionOutcome> {
  // Intercept orchestrator tools (e.g. delegate) — handled by the runner, not the registry
  const meta = ctx.tools.getMetadata(name)
  if (meta?.orchestrator_intercept) {
    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call',
      callId,
      name,
      arguments: JSON.stringify(args),
      turnNumber: ctx.turnNumber,
    })
    ctx.events.emit({
      type: EVENT_TYPES.TOOL_STARTED,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { callId, name, args, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })
    const delegateStartMs = Date.now()
    const deps: OrchestratorDeps = {
      agents: ctx.agents,
      items: ctx.items,
      toolOutputs: ctx.toolOutputs,
      preferences: ctx.preferences,
      provider: ctx.provider,
      tools: ctx.tools,
      events: ctx.events,
      agentDefinitions: ctx.agentDefinitions,
    }
    return handleDelegation(callId, args, ctx, deps, delegateStartMs)
  }

  // Hydrate args (auto-populate tool_outputs.* fields)
  const lastOutputId = await getLastOutputId(ctx)
  const hydratedArgs = hydrateToolArgs(name, args, lastOutputId)

  // Check if tool requires approval (with override resolution)
  const needsApproval = await resolveRequiresApproval(ctx, name, meta?.requires_approval ?? false)
  if (needsApproval) {
    // Save the function_call item
    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call',
      callId,
      name,
      arguments: JSON.stringify(hydratedArgs),
      saveOutput: save ?? null,
      turnNumber: ctx.turnNumber,
    })

    // Park agent in waiting state
    const waitEntry: WaitingFor = {
      callId,
      type: 'approval',
      name,
      args: hydratedArgs,
      description: `Approve execution of ${name}?`,
    }

    const waiting = waitForMany(ctx.agent, [waitEntry])
    ctx.agent = await ctx.agents.update(ctx.agent.id, {
      status: 'waiting',
      waitingFor: waiting.waitingFor,
    })

    ctx.events.emit({
      type: EVENT_TYPES.TOOL_PROPOSED,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { callId, name, args: hydratedArgs, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })

    ctx.events.emit({
      type: EVENT_TYPES.AGENT_WAITING,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { waitingFor: waiting.waitingFor, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })

    return { type: 'waiting', waiting_for: waiting.waitingFor }
  }

  // Execute directly
  ctx.events.emit({
    type: EVENT_TYPES.TOOL_STARTED,
    agent_id: ctx.agent.id,
    session_id: ctx.agent.sessionId,
    payload: { callId, name, args: hydratedArgs, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
    timestamp: Date.now(),
  })

  // Save function_call item
  await ctx.items.create({
    agentId: ctx.agent.id,
    type: 'function_call',
    callId,
    name,
    arguments: JSON.stringify(hydratedArgs),
    saveOutput: save ?? null,
    turnNumber: ctx.turnNumber,
  })

  const startMs = Date.now()
  const result = await ctx.tools.execute(name, hydratedArgs, {
    agent_id: ctx.agent.id,
    session_id: ctx.agent.sessionId,
    signal: ctx.signal,
  })
  const durationMs = Date.now() - startMs

  const outputStr = formatToolOutput(result)

  // Save function_call_output item
  await ctx.items.create({
    agentId: ctx.agent.id,
    type: 'function_call_output',
    callId,
    output: outputStr,
    contentBlocks: result.content_blocks ?? null,
    isError: !result.ok,
    turnNumber: ctx.turnNumber,
    durationMs,
  })

  // Persist large outputs to tool output store
  if (result.ok && save && !isToolOutputsTool(name)) {
    await ctx.toolOutputs.save({
      agentId: ctx.agent.id,
      callId,
      toolName: name,
      data: result.output,
    })
  }

  ctx.events.emit({
    type: EVENT_TYPES.TOOL_COMPLETED,
    agent_id: ctx.agent.id,
    session_id: ctx.agent.sessionId,
    payload: { callId, name, success: result.ok, output: outputStr, durationMs, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
    timestamp: Date.now(),
  })

  return { type: 'continue' }
}

// ---------------------------------------------------------------------------
// Batch tool execution
// ---------------------------------------------------------------------------

async function executeToolBatch(
  ctx: RunContext,
  specs: Array<ToolCallSpec & { callId: string }>,
): Promise<StepExecutionOutcome> {
  const lastOutputId = await getLastOutputId(ctx)

  // Separate orchestrator-intercepted tools, tools needing approval, and directly executable ones
  const interceptedSpecs: Array<ToolCallSpec & { callId: string }> = []
  const needsApproval: Array<ToolCallSpec & { callId: string }> = []
  const canExecute: Array<ToolCallSpec & { callId: string }> = []

  for (const spec of specs) {
    const meta = ctx.tools.getMetadata(spec.tool)
    if (meta?.orchestrator_intercept) {
      interceptedSpecs.push(spec)
    } else if (await resolveRequiresApproval(ctx, spec.tool, meta?.requires_approval ?? false)) {
      needsApproval.push(spec)
    } else {
      canExecute.push(spec)
    }
  }

  // Hydrate args once and save function_call items for ALL tools
  const hydratedMap = new Map<string, Record<string, unknown>>()
  for (const spec of specs) {
    const hydrated = hydrateToolArgs(spec.tool, spec.args, lastOutputId)
    hydratedMap.set(spec.callId, hydrated)
    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call',
      callId: spec.callId,
      name: spec.tool,
      arguments: JSON.stringify(hydrated),
      saveOutput: spec.save ?? null,
      turnNumber: ctx.turnNumber,
    })
  }

  // Execute orchestrator-intercepted tools sequentially (e.g. delegate spawns child agents)
  if (interceptedSpecs.length > 0) {
    const deps: OrchestratorDeps = {
      agents: ctx.agents,
      items: ctx.items,
      toolOutputs: ctx.toolOutputs,
      preferences: ctx.preferences,
      provider: ctx.provider,
      tools: ctx.tools,
      events: ctx.events,
      agentDefinitions: ctx.agentDefinitions,
    }
    for (const spec of interceptedSpecs) {
      ctx.events.emit({
        type: EVENT_TYPES.TOOL_STARTED,
        agent_id: ctx.agent.id,
        session_id: ctx.agent.sessionId,
        payload: { callId: spec.callId, name: spec.tool, args: spec.args, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
        timestamp: Date.now(),
      })
      const interceptStartMs = Date.now()
      const outcome = await handleDelegation(spec.callId, spec.args, ctx, deps, interceptStartMs)
      if (outcome.type === 'waiting') {
        return outcome
      }
    }
  }

  // Execute tools that don't need approval
  if (canExecute.length > 0) {
    const toolCalls: ToolCall[] = canExecute.map((spec) => ({
      call_id: spec.callId,
      name: spec.tool,
      args: hydratedMap.get(spec.callId) ?? spec.args,
      save: spec.save,
    }))

    for (const tc of toolCalls) {
      ctx.events.emit({
        type: EVENT_TYPES.TOOL_STARTED,
        agent_id: ctx.agent.id,
        session_id: ctx.agent.sessionId,
        payload: { callId: tc.call_id, name: tc.name, args: tc.args, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
        timestamp: Date.now(),
      })
    }

    const batchResult = await ctx.tools.executeBatch(toolCalls, {
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      signal: ctx.signal,
    })

    for (const res of batchResult.results) {
      const outputStr = formatToolOutput(res)
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'function_call_output',
        callId: res.call_id,
        output: outputStr,
        contentBlocks: res.content_blocks ?? null,
        isError: !res.ok,
        turnNumber: ctx.turnNumber,
      })

      const spec = canExecute.find((s) => s.callId === res.call_id)
      if (res.ok && spec?.save && !isToolOutputsTool(spec.tool)) {
        await ctx.toolOutputs.save({
          agentId: ctx.agent.id,
          callId: res.call_id,
          toolName: spec.tool,
          data: res.output,
        })
      }

      ctx.events.emit({
        type: EVENT_TYPES.TOOL_COMPLETED,
        agent_id: ctx.agent.id,
        session_id: ctx.agent.sessionId,
        payload: { callId: res.call_id, name: spec?.tool ?? 'unknown', success: res.ok, output: outputStr, durationMs: 0, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
        timestamp: Date.now(),
      })
    }
  }

  // If any tools need approval, park the agent
  if (needsApproval.length > 0) {
    const waitEntries: WaitingFor[] = needsApproval.map((spec) => ({
      callId: spec.callId,
      type: 'approval' as const,
      name: spec.tool,
      args: hydratedMap.get(spec.callId) ?? spec.args,
      description: `Approve execution of ${spec.tool}?`,
    }))

    for (const entry of waitEntries) {
      ctx.events.emit({
        type: EVENT_TYPES.TOOL_PROPOSED,
        agent_id: ctx.agent.id,
        session_id: ctx.agent.sessionId,
        payload: { callId: entry.callId, name: entry.name, args: entry.args ?? {}, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
        timestamp: Date.now(),
      })
    }

    const waiting = waitForMany(ctx.agent, waitEntries)
    ctx.agent = await ctx.agents.update(ctx.agent.id, {
      status: 'waiting',
      waitingFor: waiting.waitingFor,
    })

    ctx.events.emit({
      type: EVENT_TYPES.AGENT_WAITING,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { waitingFor: waiting.waitingFor, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })

    return { type: 'waiting', waiting_for: waiting.waitingFor }
  }

  return { type: 'continue' }
}

// ---------------------------------------------------------------------------
// Ask user
// ---------------------------------------------------------------------------

async function executeAskUser(
  ctx: RunContext,
  question: string,
  contextStr?: string,
): Promise<StepExecutionOutcome> {
  const callId = randomUUID()

  await ctx.items.create({
    agentId: ctx.agent.id,
    type: 'message',
    role: 'assistant',
    content: contextStr ? `${question}\n\nContext: ${contextStr}` : question,
    callId,
    turnNumber: ctx.turnNumber,
  })

  const waitEntry: WaitingFor = {
    callId,
    type: 'human',
    name: 'user_response',
    description: question,
  }

  const waiting = waitForMany(ctx.agent, [waitEntry])
  ctx.agent = await ctx.agents.update(ctx.agent.id, {
    status: 'waiting',
    waitingFor: waiting.waitingFor,
  })

  ctx.events.emit({
    type: EVENT_TYPES.AGENT_WAITING,
    agent_id: ctx.agent.id,
    session_id: ctx.agent.sessionId,
    payload: { waitingFor: waiting.waitingFor, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
    timestamp: Date.now(),
  })

  return { type: 'waiting', waiting_for: waiting.waitingFor }
}

// ---------------------------------------------------------------------------
// Delegation — spawn and run a child agent
// ---------------------------------------------------------------------------

async function handleDelegation(
  callId: string,
  args: Record<string, unknown>,
  ctx: RunContext,
  deps: OrchestratorDeps,
  startMs: number = Date.now(),
): Promise<StepExecutionOutcome> {
  const task = args.task as string
  if (!task) {
    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call_output',
      callId,
      output: 'delegate requires "task"',
      isError: true,
      turnNumber: ctx.turnNumber,
    })
    return { type: 'continue' }
  }

  // Depth guard
  if (ctx.agent.depth + 1 > MAX_AGENT_DEPTH) {
    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call_output',
      callId,
      output: `Max agent depth (${MAX_AGENT_DEPTH}) exceeded. Cannot delegate further.`,
      isError: true,
      turnNumber: ctx.turnNumber,
    })
    return { type: 'continue' }
  }

  // Resolve named agent definition — explicit name, then "default", then hardcoded fallback
  const agentName = typeof args.agent === 'string' ? args.agent : undefined
  const available = ctx.agentDefinitions.list().filter((d) => d.name !== 'default')

  if (agentName) {
    const exists = ctx.agentDefinitions.get(agentName)
    if (!exists) {
      const names = available.map((d) => d.name).join(', ') || 'none'
      await ctx.items.create({
        agentId: ctx.agent.id,
        type: 'function_call_output',
        callId,
        output: `Unknown agent: "${agentName}". Available agents: ${names}`,
        isError: true,
        turnNumber: ctx.turnNumber,
      })
      return { type: 'continue' }
    }
  }

  const definition = agentName
    ? ctx.agentDefinitions.get(agentName)
    : ctx.agentDefinitions.get('default')

  const childModel = definition?.model ?? 'anthropic:claude-haiku-4-5-20251001'
  const childProvider = childModel.includes(':') ? childModel.slice(0, childModel.indexOf(':')) : childModel
  const childMaxTurns = definition?.max_turns ?? 10

  // Create child agent
  const child = await ctx.agents.create({
    sessionId: ctx.agent.sessionId,
    parentId: ctx.agent.id,
    sourceCallId: callId,
    depth: ctx.agent.depth + 1,
    task,
    config: {
      ...ctx.agent.config,
      model: childModel,
      provider: childProvider,
      ...(definition?.system_prompt ? { system_prompt: definition.system_prompt } : {}),
      ...(definition?.tools ? { allowed_tools: definition.tools } : {}),
    },
  })

  // Seed child with task as user message
  await ctx.items.create({
    agentId: child.id,
    type: 'message',
    role: 'user',
    content: task,
    turnNumber: 0,
  })

  deps.events.emit({
    type: EVENT_TYPES.AGENT_STARTED,
    agent_id: child.id,
    session_id: ctx.agent.sessionId,
    payload: { task, model: ctx.agent.config.model, parentId: ctx.agent.id, depth: child.depth },
    timestamp: Date.now(),
  })

  // Resolve the correct provider for the child's model (may differ from parent)
  const childDeps = deps.providers
    ? { ...deps, provider: deps.providers.resolve(childModel) }
    : deps

  // Run child synchronously (blocking parent)
  const childResult = await runAgent(child.id, childDeps, {
    maxTurns: childMaxTurns,
    signal: ctx.signal,
    stream: ctx.stream,
  })

  // Handle child completion
  if (childResult.status === 'completed') {
    const output = childResult.result ?? '(child completed with no output)'
    await ctx.items.create({
      agentId: ctx.agent.id,
      type: 'function_call_output',
      callId,
      output,
      isError: false,
      turnNumber: ctx.turnNumber,
    })
    deps.events.emit({
      type: EVENT_TYPES.TOOL_COMPLETED,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { callId, name: 'delegate', success: true, output, durationMs: Date.now() - startMs, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })
    return { type: 'continue' }
  }

  // Handle child waiting — propagate wait up to parent
  if (childResult.status === 'waiting' && childResult.waitingFor) {
    const waitEntry: WaitingFor = {
      callId,
      type: 'agent',
      name: 'delegate',
      description: `Waiting for child agent to complete: ${task.slice(0, 100)}`,
    }

    const waiting = waitForMany(ctx.agent, [waitEntry])
    ctx.agent = await ctx.agents.update(ctx.agent.id, {
      status: 'waiting',
      waitingFor: waiting.waitingFor,
    })

    deps.events.emit({
      type: EVENT_TYPES.AGENT_WAITING,
      agent_id: ctx.agent.id,
      session_id: ctx.agent.sessionId,
      payload: { waitingFor: waiting.waitingFor, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
      timestamp: Date.now(),
    })

    return { type: 'waiting', waiting_for: waiting.waitingFor }
  }

  // Child failed
  const errorMsg = childResult.error ?? 'Child agent failed'
  await ctx.items.create({
    agentId: ctx.agent.id,
    type: 'function_call_output',
    callId,
    output: `Delegation failed: ${errorMsg}`,
    isError: true,
    turnNumber: ctx.turnNumber,
  })
  deps.events.emit({
    type: EVENT_TYPES.TOOL_COMPLETED,
    agent_id: ctx.agent.id,
    session_id: ctx.agent.sessionId,
    payload: { callId, name: 'delegate', success: false, output: `Delegation failed: ${errorMsg}`, durationMs: Date.now() - startMs, parentId: ctx.agent.parentId, depth: ctx.agent.depth },
    timestamp: Date.now(),
  })
  return { type: 'continue' }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function isToolOutputsTool(name: string): boolean {
  return name.startsWith('tool_outputs.')
}

async function getLastOutputId(ctx: RunContext): Promise<string | undefined> {
  return ctx.toolOutputs.getLastId(ctx.agent.id)
}
