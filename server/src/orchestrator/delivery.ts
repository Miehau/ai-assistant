import type { OrchestratorDeps, RunResult } from './types.js'
import { deliverOne, completeAgent } from '../domain/agent.js'
import { runAgent } from './runner.js'
import { materializeTextOutput, materializeToolOutput } from './output.js'
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

  const storedOutput = await materializeTextOutput(output, {
    sessionFilesRoot: deps.sessionFilesRoot,
    inlineLimitBytes: deps.inlineOutputLimitBytes,
    sessionId: agent.sessionId,
    agentId,
    callId,
    toolName: 'external-delivery',
    extension: 'txt',
  })

  // Store the delivered result as a function_call_output item
  await deps.items.create({
    agentId,
    type: 'function_call_output',
    callId,
    output: storedOutput,
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

export type ApprovalScope = 'once' | 'conversation' | 'always'

export async function deliverApproval(
  agentId: string,
  callId: string,
  decision: 'approved' | 'denied',
  deps: OrchestratorDeps,
  scope?: ApprovalScope,
): Promise<RunResult> {
  const release = await agentLock.acquire(agentId)
  try {
    return await deliverApprovalLocked(agentId, callId, decision, deps, scope)
  } finally {
    release()
  }
}

const APPROVAL_PREFIX_SESSION = 'tool_approval_session:'
const APPROVAL_PREFIX_GLOBAL = 'tool_approval_global:'

async function deliverApprovalLocked(
  agentId: string,
  callId: string,
  decision: 'approved' | 'denied',
  deps: OrchestratorDeps,
  scope?: ApprovalScope,
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

    // Write denial outputs for any remaining pending approvals in this agent
    const remainingWaits = agent.waitingFor.filter(
      (w) => w.callId !== callId && w.type === 'approval',
    )
    for (const remaining of remainingWaits) {
      await deps.items.create({
        agentId,
        type: 'function_call_output',
        callId: remaining.callId,
        output: 'Tool execution denied by user (batch cancelled)',
        isError: true,
        turnNumber: agent.turnCount,
      })
      deps.events.emit({
        type: EVENT_TYPES.TOOL_DENIED,
        agent_id: agentId,
        session_id: agent.sessionId,
        payload: { callId: remaining.callId, name: remaining.name },
        timestamp: Date.now(),
      })
    }

    // Complete the agent — denial means "stop", not "try again"
    const stoppingMessage =
      "Okay, stopping since the tool request wasn't approved. Let me know how you'd like to continue."

    await deps.items.create({
      agentId,
      type: 'message',
      role: 'assistant',
      content: stoppingMessage,
      turnNumber: agent.turnCount,
    })

    const completed = completeAgent({ ...agent, status: 'running', waitingFor: [] }, stoppingMessage)
    await deps.agents.update(agentId, {
      status: 'completed',
      result: completed.result,
      waitingFor: [],
      completedAt: completed.completedAt,
    })

    deps.events.emit({
      type: EVENT_TYPES.AGENT_COMPLETED,
      agent_id: agentId,
      session_id: agent.sessionId,
      payload: { result: stoppingMessage, parentId: agent.parentId, depth: agent.depth },
      timestamp: Date.now(),
    })

    // Propagate denial up the parent chain so parent agents don't stay
    // stuck in 'waiting' status.  Without this, new messages to a session
    // whose root agent is still 'waiting' get a 202 instead of running.
    if (agent.parentId && agent.sourceCallId) {
      return propagateDenialUp(agent, stoppingMessage, deps)
    }

    return {
      agentId,
      status: 'completed',
      result: stoppingMessage,
      turnCount: agent.turnCount,
    }
  }

  // Approved: persist approval override if scope is broader than "once"
  if (scope && scope !== 'once') {
    const toolName = waitEntry.name
    try {
      if (scope === 'conversation') {
        const key = `${APPROVAL_PREFIX_SESSION}${agent.sessionId}:${toolName}`
        await deps.preferences.set(key, 'false')
      } else if (scope === 'always') {
        const key = `${APPROVAL_PREFIX_GLOBAL}${toolName}`
        await deps.preferences.set(key, 'false')
      }
    } catch {
      // Non-fatal — approval still proceeds, just won't be remembered
    }
  }

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

  const outputStr = await materializeToolOutput(result, {
    sessionFilesRoot: deps.sessionFilesRoot,
    inlineLimitBytes: deps.inlineOutputLimitBytes,
    sessionId: agent.sessionId,
    agentId,
    callId,
    toolName,
  })

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
// Propagate a denied subagent's completion up the parent chain
// ---------------------------------------------------------------------------

async function propagateDenialUp(
  child: { id: string; parentId: string | null; sourceCallId: string | null; sessionId: string; depth: number },
  denialResult: string,
  deps: OrchestratorDeps,
): Promise<RunResult> {
  let currentChild = child

  while (currentChild.parentId && currentChild.sourceCallId) {
    const parent = await deps.agents.getById(currentChild.parentId)
    if (!parent || parent.status !== 'waiting') break

    const parentWait = parent.waitingFor.find((w) => w.callId === currentChild.sourceCallId)
    if (!parentWait) break

    // Deliver the denial result as the delegate tool's output (error)
    await deps.items.create({
      agentId: parent.id,
      type: 'function_call_output',
      callId: currentChild.sourceCallId,
      output: denialResult,
      isError: true,
      turnNumber: parent.turnCount,
    })

    deps.events.emit({
      type: EVENT_TYPES.TOOL_COMPLETED,
      agent_id: parent.id,
      session_id: parent.sessionId,
      payload: {
        callId: currentChild.sourceCallId,
        name: 'delegate',
        success: false,
        output: denialResult,
        durationMs: 0,
        parentId: parent.parentId ?? null,
        depth: parent.depth,
      },
      timestamp: Date.now(),
    })

    // Transition parent — remove this callId from waitingFor
    const updated = deliverOne(parent, currentChild.sourceCallId)
    await deps.agents.update(parent.id, {
      status: updated.status,
      waitingFor: updated.waitingFor,
    })

    if (updated.status === 'running') {
      // Parent is ready to resume — use the normal propagation path
      return runAndPropagateUp(parent.id, deps)
    }

    if (updated.status !== 'waiting') {
      return {
        agentId: parent.id,
        status: updated.status,
        waitingFor: updated.waitingFor,
        turnCount: updated.turnCount,
      }
    }

    // Parent is still waiting for other deliveries — continue up the chain
    // only if the parent itself is a subagent
    const parentAgent = await deps.agents.getById(parent.id)
    if (!parentAgent?.parentId || !parentAgent.sourceCallId) break
    currentChild = parentAgent
  }

  return {
    agentId: child.id,
    status: 'completed' as const,
    result: denialResult,
    turnCount: 0,
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

  // Resolve the correct provider for this agent's model (may differ from the
  // original request's provider when delegation crosses provider boundaries).
  const resolveDepsForAgent = async (agentId: string): Promise<OrchestratorDeps> => {
    if (!deps.providers) return deps
    const agent = await deps.agents.getById(agentId)
    if (!agent) return deps
    try {
      return { ...deps, provider: deps.providers.resolve(agent.config.model) }
    } catch {
      return deps // fall back to original provider if resolution fails
    }
  }

  // Always stream resumed runs so TEXT_DELTA events flow through the event bus
  // to the SSE subscription the frontend opens before sending approval/deliver.
  let agentDeps = await resolveDepsForAgent(currentId)
  let result = await runAgent(currentId, agentDeps, { stream: true })

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
    const rawDelegateOutput = result.result ?? '(completed)'
    const delegateOutput = await materializeTextOutput(rawDelegateOutput, {
      sessionFilesRoot: deps.sessionFilesRoot,
      inlineLimitBytes: deps.inlineOutputLimitBytes,
      sessionId: parent.sessionId,
      agentId: parent.id,
      callId: agent.sourceCallId,
      toolName: 'delegate',
      extension: 'md',
      persistEvenWhenInline: true,
    })
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

    // Continue the loop — run the parent with its own provider
    currentId = parent.id
    agentDeps = await resolveDepsForAgent(currentId)
    result = await runAgent(currentId, agentDeps, { stream: true })
  }

  return result
}
