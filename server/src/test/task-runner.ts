import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'

import { loadConfig } from '../lib/config.js'
import { initRuntime, shutdownRuntime, type RuntimeContext } from '../lib/runtime.js'
import type { LLMMessage, LLMRequest } from '../providers/types.js'
import { buildDeps, prepareSessionTurn } from '../services/session-runner.js'
import { runAgent } from '../orchestrator/runner.js'
import { listTasks } from '../tasks/storage.js'

async function main() {
  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'task-runner-test-'))
  process.env.DATABASE_URL = path.join(tmpDir, 'test.db')
  process.env.TASKS_DIR = path.join(tmpDir, 'tasks')
  process.env.WORKSPACE_DIR = path.join(tmpDir, 'workspace')
  process.env.SESSION_FILES_DIR = path.join(tmpDir, 'sessions')
  process.env.ENCRYPTION_KEY = 'test-encryption-key'
  delete process.env.ANTHROPIC_API_KEY
  delete process.env.OPENAI_API_KEY
  delete process.env.OLLAMA_BASE_URL
  delete process.env.OPENROUTER_API_KEY

  let runtime: RuntimeContext | null = null
  try {
    runtime = await initRuntime(loadConfig())
    const user = await runtime.repositories.users.create({
      email: 'task-runner@test.local',
      apiKeyHash: createHash('sha256').update('task-runner-test-key').digest('hex'),
    })

    const stubProvider = {
      async generate(request: LLMRequest) {
        const joined = request.messages.map(messageText).join('\n')
        if (joined.includes('Background task completed.')) {
          return {
            content: 'Callback saw completion and is ready to notify the user.',
            usage: { input_tokens: 1, output_tokens: 1 },
            finish_reason: 'stop',
          }
        }
        if (joined.includes('Background task ID:')) {
          return {
            content: '# Result\n\nThe background task produced a durable markdown result.',
            usage: { input_tokens: 1, output_tokens: 1 },
            finish_reason: 'stop',
          }
        }
        return {
          content: 'Initial planner reply.',
          usage: { input_tokens: 1, output_tokens: 1 },
          finish_reason: 'stop',
        }
      },
      async *stream() {
        yield {
          type: 'done' as const,
          response: {
            content: 'stream unused',
            usage: { input_tokens: 1, output_tokens: 1 },
            finish_reason: 'stop',
          },
        }
      },
    }
    runtime.providers.register('openrouter', stubProvider)

    const prepared = await prepareSessionTurn(runtime, {
      userId: user.id,
      agent: 'planner',
      input: 'Set up a callback-capable planner session.',
    })
    await runAgent(prepared.agent.id, buildDeps(runtime, prepared.model))

    const enqueue = await runtime.tools.execute('tasks.enqueue', {
      title: 'Background markdown task',
      owner: 'researcher',
      output_profile: 'generic',
      body: 'Produce a concise markdown result.',
    }, {
      agent_id: prepared.agent.id,
      session_id: prepared.sessionId,
      signal: AbortSignal.timeout(5000),
    })

    assert.equal(enqueue.ok, true)

    const taskId = (enqueue.output as { id: string }).id
    const completed = await waitForTask(runtime, taskId, 'done')
    assert.equal(completed.frontmatter.callback_agent_id, prepared.agent.id)
    assert.equal(completed.frontmatter.callback_session_id, prepared.sessionId)
    assert.match(completed.frontmatter.output_note ?? '', /^@note\//)
    assert.match(completed.frontmatter.output_artifact ?? '', /^artifact:\/\//)

    const callbackItems = await runtime.repositories.items.listByAgent(prepared.agent.id)
    assert(callbackItems.some((item) => item.role === 'user' && item.content?.includes(`Task ID: ${taskId}`)))
    assert(callbackItems.some((item) => item.role === 'assistant' && item.content === 'Callback saw completion and is ready to notify the user.'))

    console.log('task-runner tests passed')
  } finally {
    if (runtime) await shutdownRuntime(runtime)
    await fs.rm(tmpDir, { recursive: true, force: true })
  }
}

async function waitForTask(runtime: RuntimeContext, taskId: string, status: string) {
  const deadline = Date.now() + 8000
  while (Date.now() < deadline) {
    const task = (await listTasks(runtime.tasksDir)).find((candidate) => candidate.frontmatter.id === taskId)
    if (task?.frontmatter.status === status) return task
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
  const task = (await listTasks(runtime.tasksDir)).find((candidate) => candidate.frontmatter.id === taskId)
  throw new Error(`Timed out waiting for task ${taskId} to reach ${status}; current status is ${task?.frontmatter.status ?? 'missing'}`)
}

function messageText(message: LLMMessage): string {
  if (typeof message.content === 'string') return message.content
  return message.content.map((block) => block.type === 'text' ? block.text : '').join('\n')
}

main()
