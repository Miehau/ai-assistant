import type { Agent, AgentConfig, AgentStatus, WaitingFor } from './types'

interface CreateAgentInput {
  id: string
  sessionId: string
  parentId?: string | null
  sourceCallId?: string | null
  depth?: number
  task: string
  config: AgentConfig
}

function now(): number {
  return Date.now()
}

function transition(agent: Agent, to: AgentStatus, allowed: AgentStatus[]): void {
  if (!allowed.includes(agent.status)) {
    throw new Error(
      `Invalid agent transition: cannot move from '${agent.status}' to '${to}'`
    )
  }
}

export function createAgent(input: CreateAgentInput): Agent {
  const timestamp = now()
  return {
    id: input.id,
    sessionId: input.sessionId,
    parentId: input.parentId ?? null,
    sourceCallId: input.sourceCallId ?? null,
    depth: input.depth ?? 0,
    task: input.task,
    config: input.config,
    status: 'pending',
    waitingFor: [],
    result: null,
    error: null,
    turnCount: 0,
    plan: null,
    createdAt: timestamp,
    updatedAt: timestamp,
    completedAt: null,
  }
}

export function startAgent(agent: Agent): Agent {
  transition(agent, 'running', ['pending', 'waiting'])
  return {
    ...agent,
    status: 'running',
    updatedAt: now(),
  }
}

export function waitForMany(agent: Agent, waitingFor: WaitingFor[]): Agent {
  transition(agent, 'waiting', ['running'])
  if (waitingFor.length === 0) {
    throw new Error('waitForMany requires at least one WaitingFor entry')
  }
  return {
    ...agent,
    status: 'waiting',
    waitingFor: [...waitingFor],
    updatedAt: now(),
  }
}

export function deliverOne(agent: Agent, callId: string): Agent {
  transition(agent, 'running', ['waiting'])
  const remaining = agent.waitingFor.filter((w) => w.callId !== callId)
  if (remaining.length === agent.waitingFor.length) {
    throw new Error(`No waiting entry found for callId '${callId}'`)
  }
  const nextStatus: AgentStatus = remaining.length === 0 ? 'running' : 'waiting'
  return {
    ...agent,
    status: nextStatus,
    waitingFor: remaining,
    updatedAt: now(),
  }
}

export function completeAgent(agent: Agent, result: string): Agent {
  transition(agent, 'completed', ['running'])
  const timestamp = now()
  return {
    ...agent,
    status: 'completed',
    result,
    updatedAt: timestamp,
    completedAt: timestamp,
  }
}

export function failAgent(agent: Agent, error: string): Agent {
  transition(agent, 'failed', ['pending', 'running', 'waiting'])
  const timestamp = now()
  return {
    ...agent,
    status: 'failed',
    error,
    updatedAt: timestamp,
    completedAt: timestamp,
  }
}

export function cancelAgent(agent: Agent): Agent {
  transition(agent, 'cancelled', ['pending', 'running', 'waiting'])
  const timestamp = now()
  return {
    ...agent,
    status: 'cancelled',
    updatedAt: timestamp,
    completedAt: timestamp,
  }
}
