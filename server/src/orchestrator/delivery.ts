import type { OrchestratorDeps, RunResult } from './types.js'
import { deliverOne } from '../domain/agent.js'
import { runAgent } from './runner.js'
import { EVENT_TYPES } from '../events/types.js'

// ---------------------------------------------------------------------------
// Deliver an external result to a waiting agent
// ---------------------------------------------------------------------------

export async function deliverResult(
  agentId: string,
  callId: string,
  output: string,
  isError: boolean,
  deps: OrchestratorDeps,
): Promise<RunResult> {
  const agent = await deps.agents.getById(agentId)
  if (!agent) throw new Error(`Agent not found: ${agentId}`)
  if (agent.status !== 'waiting') {
    throw new Error(`Agent ${agentId} is not waiting (status: ${agent.status})`)
  }

  const waitEntry = agent.waitingFor.find((w) => w.callId === callId)
  if (!waitEntry) {
    throw new Error(`Agent ${agentId} is not waiting for callId '${callId}'`)
  }

  // Store the delivered result as a function_call_output item
  await deps.items.create({
    agentId,
    type: 'function_call_output',
    callId,
    output,
    isError,
    turnNumber: agent.turnCount,
  })

  // Update agent state — remove this callId from waitingFor
  const updated = deliverOne(agent, callId)
  await deps.agents.update(agentId, {
    status: updated.status,
    waitingFor: updated.waitingFor,
  })

  // If no more pending waits, resume the agent
  if (updated.status === 'running') {
    const result = await runAgent(agentId, deps)

    // Auto-propagate: if this agent has a parent and completed, deliver result upward
    const agent = await deps.agents.getById(agentId)
    if (
      agent &&
      result.status === 'completed' &&
      agent.parentId &&
      agent.sourceCallId
    ) {
      return deliverResult(
        agent.parentId,
        agent.sourceCallId,
        result.result ?? '(completed)',
        false,
        deps,
      )
    }

    return result
  }

  return {
    agentId,
    status: updated.status,
    waitingFor: updated.waitingFor,
    turnCount: updated.turnCount,
  }
}

// ---------------------------------------------------------------------------
// Deliver an approval or denial for a tool requiring user confirmation
// ---------------------------------------------------------------------------

export async function deliverApproval(
  agentId: string,
  callId: string,
  decision: 'approved' | 'denied',
  deps: OrchestratorDeps,
): Promise<RunResult> {
  const agent = await deps.agents.getById(agentId)
  if (!agent) throw new Error(`Agent not found: ${agentId}`)
  if (agent.status !== 'waiting') {
    throw new Error(`Agent ${agentId} is not waiting (status: ${agent.status})`)
  }

  const waitEntry = agent.waitingFor.find((w) => w.callId === callId)
  if (!waitEntry) {
    throw new Error(`Agent ${agentId} is not waiting for callId '${callId}'`)
  }

  if (decision === 'denied') {
    // Write denial as an error output
    await deps.items.create({
      agentId,
      type: 'function_call_output',
      callId,
      output: 'Tool execution denied by user',
      isError: true,
      turnNumber: agent.turnCount,
    })

    deps.events.emit({
      type: EVENT_TYPES.TOOL_DENIED,
      agent_id: agentId,
      session_id: agent.sessionId,
      payload: { callId, name: waitEntry.name },
      timestamp: Date.now(),
    })

    const updated = deliverOne(agent, callId)
    await deps.agents.update(agentId, {
      status: updated.status,
      waitingFor: updated.waitingFor,
    })

    if (updated.status === 'running') {
      return runAgent(agentId, deps)
    }

    return {
      agentId,
      status: updated.status,
      waitingFor: updated.waitingFor,
      turnCount: updated.turnCount,
    }
  }

  // Approved: execute the tool, then resume

  deps.events.emit({
    type: EVENT_TYPES.TOOL_APPROVED,
    agent_id: agentId,
    session_id: agent.sessionId,
    payload: { callId, name: waitEntry.name },
    timestamp: Date.now(),
  })

  // Find the function_call item to get tool name and args
  const items = await deps.items.listByAgent(agentId)
  const callItem = items.find(
    (i) => i.type === 'function_call' && i.callId === callId,
  )
  if (!callItem) {
    throw new Error(`No function_call item found for callId '${callId}'`)
  }

  const toolName = callItem.name!
  const toolArgs: Record<string, unknown> = callItem.arguments
    ? JSON.parse(callItem.arguments)
    : {}

  // Emit TOOL_STARTED so the UI shows activity
  deps.events.emit({
    type: EVENT_TYPES.TOOL_STARTED,
    agent_id: agentId,
    session_id: agent.sessionId,
    payload: { callId, name: toolName, args: toolArgs, parentId: agent.parentId, depth: agent.depth },
    timestamp: Date.now(),
  })

  // Execute the tool
  const startMs = Date.now()
  const result = await deps.tools.execute(toolName, toolArgs, {
    agent_id: agentId,
    session_id: agent.sessionId,
    signal: AbortSignal.timeout(agent.config.tool_execution_timeout_ms),
  })
  const durationMs = Date.now() - startMs

  // Store the output
  const outputStr = result.ok
    ? typeof result.output === 'string'
      ? result.output
      : JSON.stringify(result.output)
    : result.error ?? 'Unknown error'

  await deps.items.create({
    agentId,
    type: 'function_call_output',
    callId,
    output: outputStr,
    isError: !result.ok,
    turnNumber: agent.turnCount,
    durationMs,
  })

  deps.events.emit({
    type: EVENT_TYPES.TOOL_COMPLETED,
    agent_id: agentId,
    session_id: agent.sessionId,
    payload: { callId, name: toolName, success: result.ok, output: outputStr, durationMs, parentId: agent.parentId, depth: agent.depth },
    timestamp: Date.now(),
  })

  // Remove from waitingFor
  const updated = deliverOne(agent, callId)
  await deps.agents.update(agentId, {
    status: updated.status,
    waitingFor: updated.waitingFor,
  })

  if (updated.status === 'running') {
    const result = await runAgent(agentId, deps)

    // Auto-propagate: if this agent has a parent and completed, deliver result upward
    const freshAgent = await deps.agents.getById(agentId)
    if (
      freshAgent &&
      result.status === 'completed' &&
      freshAgent.parentId &&
      freshAgent.sourceCallId
    ) {
      return deliverResult(
        freshAgent.parentId,
        freshAgent.sourceCallId,
        result.result ?? '(completed)',
        false,
        deps,
      )
    }

    return result
  }

  return {
    agentId,
    status: updated.status,
    waitingFor: updated.waitingFor,
    turnCount: updated.turnCount,
  }
}
