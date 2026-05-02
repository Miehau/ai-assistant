import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'

import { createTask, listTasks, parseTaskFile, updateTask } from '../tasks/storage.js'

const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'task-storage-test-'))

try {
  const parsed = parseTaskFile([
    '---',
    'id: executable-1',
    'title: Executable task',
    'status: callback_pending',
    'kind: background',
    'owner: researcher',
    'priority: high',
    'depends_on: [task-a, task-b]',
    'callback_agent_id: agent-1',
    'callback_session_id: session-1',
    'execution_session_id: session-2',
    'execution_agent_id: agent-2',
    'output_note: @note/executable.md',
    'output_artifact: artifact://task-runner/executable-result.md',
    'output_profile: research',
    'telegram_connection_id: conn-1',
    'telegram_chat_id: 123',
    'telegram_original_message_id: 10',
    'telegram_accepted_message_id: 11',
    'created_by: agent',
    'created_at: 2026-05-02T10:00:00.000Z',
    'updated_at: 2026-05-02T10:01:00.000Z',
    '---',
    '',
    'Body',
  ].join('\n'), path.join(tmpDir, 'executable-1.md'))

  assert.equal(parsed.frontmatter.status, 'callback_pending')
  assert.equal(parsed.frontmatter.kind, 'background')
  assert.deepEqual(parsed.frontmatter.depends_on, ['task-a', 'task-b'])
  assert.equal(parsed.frontmatter.callback_agent_id, 'agent-1')
  assert.equal(parsed.frontmatter.output_note, '@note/executable.md')
  assert.equal(parsed.frontmatter.output_profile, 'research')
  assert.equal(parsed.frontmatter.telegram_accepted_message_id, '11')

  const created = await createTask(tmpDir, {
    id: 'queued-background-1',
    title: 'Queued background',
    owner: 'researcher',
    priority: 'medium',
    status: 'queued',
    kind: 'background',
    outputProfile: 'generic',
    callbackAgentId: 'creator-agent',
    callbackSessionId: 'creator-session',
    body: 'Do background work.',
    createdBy: 'agent',
  })

  assert.equal(created.frontmatter.status, 'queued')
  assert.equal(created.frontmatter.kind, 'background')
  assert.equal(created.frontmatter.callback_agent_id, 'creator-agent')

  const updated = await updateTask(tmpDir, 'queued-background-1', {
    status: 'done',
    execution_session_id: 'execution-session',
    execution_agent_id: 'execution-agent',
    output_note: '@note/queued-background.md',
    output_artifact: 'artifact://task-runner/queued-background-result.md',
    telegram_completion_message_id: '12',
    completion_note: 'Done.',
  })

  assert.equal(updated.frontmatter.status, 'done')
  assert.equal(updated.frontmatter.completed_at, updated.frontmatter.updated_at)
  assert.equal(updated.frontmatter.execution_session_id, 'execution-session')
  assert.equal(updated.frontmatter.output_note, '@note/queued-background.md')
  assert.equal(updated.frontmatter.telegram_completion_message_id, '12')

  await fs.writeFile(path.join(tmpDir, 'legacy.md'), [
    '---',
    'id: legacy',
    'title: Legacy task',
    'status: open',
    'owner: planner',
    'priority: low',
    'depends_on: []',
    'created_by: agent',
    'created_at: 2026-05-02T10:00:00.000Z',
    'updated_at: 2026-05-02T10:00:00.000Z',
    '---',
    '',
    'Legacy body.',
  ].join('\n'))

  const listed = await listTasks(tmpDir)
  assert(listed.some((task) => task.frontmatter.id === 'legacy' && task.frontmatter.status === 'open'))
  assert(listed.some((task) => task.frontmatter.id === 'queued-background-1' && task.frontmatter.status === 'done'))

  console.log('Task storage tests passed')
} finally {
  await fs.rm(tmpDir, { recursive: true, force: true })
}
