import type { OrchestratorDeps, RunResult } from './types.js'
import { deliverOne } from '../domain/agent.js'
import { runAgent } from './runner.js'
import { formatToolOutput } from './output.js'
import { EVENT_TYPES } from '../events/types.js'
import { AgentLock } from '../lib/agent-lock.js'

const agentLock = new AgentLock()

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
  const release = await agentLock.acquire(agentId)
  try {
    return await deliverResultLocked(agentId, callId, output, isError, deps)
  } finally {
    release()
  }
}

async function deliverResultLocked(
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

  // If still waiting for other deliveries, return
  if (updated.status !== 'running') {
    return {
      agentId,
      status: updated.status,
      waitingFor: updated.waitingFor,
      turnCount: updated.turnCount,
    }
  }

  // Resume the agent and propagate completion up the parent chain iteratively
  return runAndPropagateUp(agentId, deps)
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
  const release = await agentLock.acquire(agentId)
  try {
    return await deliverApprovalLocked(agentId, callId, decision, deps)
  } finally {
    release()
  }
}

async function deliverApprovalLocked(
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
      return runAndPropagateUp(agentId, deps)
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

  const outputStr = formatToolOutput(result)

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
    return runAndPropagateUp(agentId, deps)
  }

  return {
    agentId,
    status: updated.status,
    waitingFor: updated.waitingFor,
    turnCount: updated.turnCount,
  }
}

// ---------------------------------------------------------------------------
// Iterative parent propagation (replaces recursive deliverResult calls)
// ---------------------------------------------------------------------------

async function runAndPropagateUp(
  startAgentId: string,
  deps: OrchestratorDeps,
): Promise<RunResult> {
  let currentId = startAgentId
  // Always stream resumed runs so TEXT_DELTA events flow through the event bus
  // to the SSE subscription the frontend opens before sending approval/deliver.
  let result = await runAgent(currentId, deps, { stream: true })

  // Walk up the parent chain: if the agent completed and has a parent, deliver the result
  while (result.status === 'completed') {
    const agent = await deps.agents.getById(currentId)
    if (!agent?.parentId || !agent.sourceCallId) break

    // Deliver result to parent
    const parent = await deps.agents.getById(agent.parentId)
    if (!parent || parent.status !== 'waiting') break

    const parentWait = parent.waitingFor.find((w) => w.callId === agent.sourceCallId)
    if (!parentWait) break

    // Store child result as parent's function_call_output
    const delegateOutput = result.result ?? '(completed)'
    await deps.items.create({
      agentId: parent.id,
      type: 'function_call_output',
      callId: agent.sourceCallId,
      output: delegateOutput,
      isError: false,
      turnNumber: parent.turnCount,
    })

    // Emit TOOL_COMPLETED for the delegate tool so the client can update its status
    deps.events.emit({
      type: EVENT_TYPES.TOOL_COMPLETED,
      agent_id: parent.id,
      session_id: parent.sessionId,
      payload: {
        callId: agent.sourceCallId,
        name: 'delegate',
        success: true,
        output: delegateOutput,
        durationMs: 0,
        parentId: parent.parentId ?? null,
        depth: parent.depth,
      },
      timestamp: Date.now(),
    })

    // Transition parent
    const updated = deliverOne(parent, agent.sourceCallId)
    await deps.agents.update(parent.id, {
      status: updated.status,
      waitingFor: updated.waitingFor,
    })

    if (updated.status !== 'running') {
      return {
        agentId: parent.id,
        status: updated.status,
        waitingFor: updated.waitingFor,
        turnCount: updated.turnCount,
      }
    }

    // Continue the loop — run the parent
    currentId = parent.id
    result = await runAgent(currentId, deps, { stream: true })
  }

  return result
}
