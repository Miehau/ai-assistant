import type { RuntimeContext } from '../lib/runtime.js'
import type { AgentConfig, Item } from '../domain/types.js'
import type { OrchestratorDeps } from '../orchestrator/types.js'

export interface PrepareSessionTurnInput {
  userId: string
  sessionId?: string
  model?: string
  agent?: string
  input: string | Item[]
  instructions?: string
  systemPrompt?: string
  mcpServerIds?: string[]
  allowedTools?: string[]
  maxTokens?: number
}

export interface PreparedSessionTurn {
  sessionId: string
  agent: Awaited<ReturnType<RuntimeContext['repositories']['agents']['getById']>> extends infer T ? NonNullable<T> : never
  model: string
  status: 'prepared' | 'active'
  output?: Item[]
}

export function resolveProvider(runtime: RuntimeContext, model: string) {
  return runtime.providers.resolve(model)
}

export function extractProviderName(model: string): string {
  const idx = model.indexOf(':')
  return idx === -1 ? model : model.slice(0, idx)
}

export function buildDeps(runtime: RuntimeContext, model: string): OrchestratorDeps {
  return {
    agents: runtime.repositories.agents,
    items: runtime.repositories.items,
    toolOutputs: runtime.repositories.toolOutputs,
    preferences: runtime.repositories.preferences,
    provider: resolveProvider(runtime, model),
    providers: runtime.providers,
    tools: runtime.tools,
    events: runtime.events,
    agentDefinitions: runtime.agentDefinitions,
    sessionFilesRoot: runtime.sessionFilesRoot,
    inlineOutputLimitBytes: runtime.inlineOutputLimitBytes,
    interceptHandlers: runtime.interceptHandlers,
  }
}

const DEFAULT_ROOT_AGENT = 'planner'

export function formatAssistantOutput(items: Item[]): Item[] {
  return items.filter((i) => i.type === 'message' && i.role === 'assistant')
}

export async function prepareSessionTurn(
  runtime: RuntimeContext,
  body: PrepareSessionTurnInput,
): Promise<PreparedSessionTurn> {
  await runtime.agentDefinitions.reload()

  const requestedAgent = body.agent?.trim() || DEFAULT_ROOT_AGENT
  const agentDef = runtime.agentDefinitions.get(requestedAgent)

  if (!agentDef) {
    const available = runtime.agentDefinitions.list().map((d) => d.name).join(', ')
    throw new Error(`Unknown agent: "${requestedAgent}". Available: ${available}`)
  }

  const model = body.model ?? agentDef?.model ?? runtime.config.defaultModel

  let sessionId = body.sessionId
  if (!sessionId) {
    const session = await runtime.repositories.sessions.create({
      userId: body.userId,
      title: typeof body.input === 'string'
        ? body.input.slice(0, 100)
        : 'New conversation',
    })
    sessionId = session.id
  } else {
    const existing = await runtime.repositories.sessions.getById(sessionId)
    if (!existing) {
      throw new Error(`Session not found: ${sessionId}`)
    }
  }

  let agent = await runtime.repositories.agents.findRootAgent(sessionId)

  if (agent && agent.status === 'pending') {
    // Reuse a pre-created root agent, e.g. for forked sessions whose history
    // was copied before the next user turn was appended.
  } else if (agent && (agent.status === 'completed' || agent.status === 'failed' || agent.status === 'cancelled')) {
    await runtime.repositories.agents.update(agent.id, {
      status: 'running',
      result: null,
      error: null,
      completedAt: null,
    })
    agent = (await runtime.repositories.agents.getById(agent.id))!
  } else if (agent && (agent.status === 'running' || agent.status === 'waiting')) {
    const items = await runtime.repositories.items.listByAgent(agent.id)
    return {
      agent,
      sessionId,
      model,
      status: 'active',
      output: formatAssistantOutput(items),
    }
  } else {
    const task = typeof body.input === 'string'
      ? body.input
      : JSON.stringify(body.input)

    const allowedTools = body.allowedTools ?? agentDef?.tools
    const mcpToolSnapshot = body.mcpServerIds?.length
      ? await runtime.mcps.getNewSessionToolSnapshot(body.mcpServerIds)
      : undefined
    const config: AgentConfig = {
      model,
      provider: extractProviderName(model),
      max_turns: agentDef?.max_turns ?? 50,
      ...(body.maxTokens ?? agentDef?.max_output_tokens
        ? { max_output_tokens: body.maxTokens ?? agentDef?.max_output_tokens }
        : {}),
      max_tool_calls_per_step: 10,
      tool_execution_timeout_ms: 60_000,
      ...(body.systemPrompt
        ? { system_prompt: body.systemPrompt }
        : agentDef?.system_prompt
          ? { system_prompt: agentDef.system_prompt }
          : {}),
      ...(allowedTools ? { allowed_tools: allowedTools } : {}),
      ...(body.mcpServerIds?.length && mcpToolSnapshot
        ? {
            tools: mcpToolSnapshot,
            tool_source_ids: body.mcpServerIds,
          }
        : {}),
    }

    agent = await runtime.repositories.agents.create({
      sessionId,
      task,
      config,
    })
    await runtime.repositories.sessions.update(sessionId, {
      rootAgentId: agent.id,
    })
  }

  if (typeof body.input === 'string') {
    await runtime.repositories.items.create({
      agentId: agent.id,
      type: 'message',
      role: 'user',
      content: body.input,
      turnNumber: agent.turnCount,
    })
  } else if (Array.isArray(body.input)) {
    for (const item of body.input) {
      await runtime.repositories.items.create({
        agentId: agent.id,
        type: item.type ?? 'message',
        role: item.role ?? 'user',
        content: item.content ?? null,
        callId: item.callId ?? null,
        name: item.name ?? null,
        arguments: item.arguments ?? null,
        output: item.output ?? null,
        isError: item.isError ?? null,
        turnNumber: agent.turnCount,
      })
    }
  }

  if (body.instructions) {
    await runtime.repositories.items.create({
      agentId: agent.id,
      type: 'message',
      role: 'system',
      content: body.instructions,
      turnNumber: agent.turnCount,
    })
  }

  return {
    agent,
    sessionId,
    model,
    status: 'prepared',
  }
}
