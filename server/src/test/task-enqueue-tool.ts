import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'

import { AgentDefinitionRegistryImpl } from '../agents/registry.js'
import { AgentEventEmitter } from '../events/emitter.js'
import { EVENT_TYPES } from '../events/types.js'
import { listTasks } from '../tasks/storage.js'
import { ToolRegistryImpl } from '../tools/registry.js'
import { registerTaskTools } from '../tools/tasks.js'

const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'task-enqueue-tool-test-'))
const tasksDir = path.join(tmpDir, 'tasks')
const workspaceDir = path.join(tmpDir, 'workspace')
const events = new AgentEventEmitter()
const registry = new ToolRegistryImpl()
const agentDefinitions = new AgentDefinitionRegistryImpl(tmpDir, [
  {
    name: 'researcher',
    description: 'Research agent',
    system_prompt: 'Research.',
  },
])

registerTaskTools(registry, tasksDir, workspaceDir, agentDefinitions)

const ctx = {
  agent_id: 'creator-agent',
  session_id: 'creator-session',
  signal: new AbortController().signal,
  events,
}

try {
  const queuedEvent = events.subscribeOnce({ types: [EVENT_TYPES.TASK_QUEUED] }, AbortSignal.timeout(1000))
  const created = await registry.execute('tasks.enqueue', {
    title: 'Find restaurants in London',
    owner: 'researcher',
    output_profile: 'research',
    body: 'Find current restaurant options and cite raw source URLs.',
  }, ctx)

  assert.equal(created.ok, true)
  const output = created.output as {
    id: string
    status: string
    callback_agent_id: string
    callback_session_id: string
    output_profile: string
  }
  assert.equal(output.status, 'queued')
  assert.equal(output.callback_agent_id, 'creator-agent')
  assert.equal(output.callback_session_id, 'creator-session')
  assert.equal(output.output_profile, 'research')

  const event = await queuedEvent
  assert.equal(event.type, EVENT_TYPES.TASK_QUEUED)
  assert.equal(event.agent_id, 'creator-agent')
  assert.equal(event.session_id, 'creator-session')
  assert.equal(event.payload.taskId, output.id)

  const tasks = await listTasks(tasksDir)
  const task = tasks.find((candidate) => candidate.frontmatter.id === output.id)
  assert(task)
  assert.equal(task.frontmatter.kind, 'background')
  assert.equal(task.frontmatter.status, 'queued')
  assert.equal(task.frontmatter.callback_agent_id, 'creator-agent')
  assert.equal(task.frontmatter.callback_session_id, 'creator-session')
  assert.equal(task.frontmatter.output_profile, 'research')

  const listed = await registry.execute('tasks.list', { status: 'queued' }, ctx)
  assert.equal(listed.ok, true)
  const listedOutput = listed.output as { counts: Record<string, number>; tasks: Array<{ id: string; output_profile?: string }> }
  assert.equal(listedOutput.counts.queued, 1)
  assert.equal(listedOutput.tasks[0]?.id, output.id)
  assert.equal(listedOutput.tasks[0]?.output_profile, 'research')

  const rejected = await registry.execute('tasks.enqueue', {
    title: 'Unknown owner',
    owner: 'missing-agent',
    body: 'This should fail.',
  }, ctx)
  assert.equal(rejected.ok, false)
  assert.match(String(rejected.error), /Unknown owner agent/)

  console.log('Task enqueue tool tests passed')
} finally {
  await fs.rm(tmpDir, { recursive: true, force: true })
}
