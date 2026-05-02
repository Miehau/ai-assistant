import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'

import { AgentEventEmitter } from '../events/emitter.js'
import { EVENT_TYPES } from '../events/types.js'
import type { RuntimeContext } from '../lib/runtime.js'
import type { StoredTelegramConnection, StoredTelegramMessageLink } from '../repositories/types.js'
import { TelegramTaskBridge } from '../services/telegram-task-bridge.js'
import { createTask, listTasks, updateTask } from '../tasks/storage.js'

const originalFetch = globalThis.fetch
const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'telegram-task-bridge-test-'))
const tasksDir = path.join(tmpDir, 'tasks')

const sentMessages: Array<Record<string, unknown>> = []
const messageLinks: StoredTelegramMessageLink[] = []
let nextTelegramMessageId = 9000

globalThis.fetch = (async (_input: string | URL | Request, init?: RequestInit) => {
  sentMessages.push(JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>)
  nextTelegramMessageId++
  return new Response(JSON.stringify({
    ok: true,
    result: { message_id: nextTelegramMessageId },
  }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })
}) as typeof fetch

const connection: StoredTelegramConnection = {
  id: 'telegram-connection',
  userId: 'user-1',
  botToken: 'plain-token',
  botUsername: 'test_bot',
  allowedTelegramUserId: '42',
  webhookPathSecret: 'path-secret',
  webhookHeaderSecret: 'header-secret',
  webhookUrl: null,
  status: 'connected',
  lastError: null,
  createdAt: Date.now(),
  updatedAt: Date.now(),
}

const events = new AgentEventEmitter()
const runtime = {
  config: { encryptionKey: undefined },
  events,
  repositories: {
    telegram: {
      async getConnection(id: string) {
        return id === connection.id ? connection : null
      },
      async createMessageLink(input: StoredTelegramMessageLink) {
        messageLinks.push(input)
      },
    },
    items: {
      async listByAgent(agentId: string) {
        return [{
          id: `assistant-${agentId}`,
          agentId,
          sequence: 1,
          type: 'message',
          role: 'assistant',
          content: `Completion for ${agentId}`,
          callId: null,
          name: null,
          arguments: null,
          output: null,
          contentBlocks: null,
          isError: null,
          saveOutput: null,
          turnNumber: 1,
          durationMs: null,
          createdAt: Date.now(),
        }]
      },
    },
  },
} as unknown as RuntimeContext

try {
  const bridge = new TelegramTaskBridge(runtime, { tasksDir })
  bridge.start()
  await Promise.resolve()

  const eventTask = await createTask(tasksDir, {
    id: 'event-completed-task',
    title: 'Event completed task',
    owner: 'researcher',
    priority: 'medium',
    status: 'queued',
    kind: 'background',
    outputProfile: 'generic',
    callbackAgentId: 'creator-agent',
    callbackSessionId: 'creator-session',
    body: 'Already done by event.',
    createdBy: 'agent',
  })
  const eventDone = await updateTask(tasksDir, eventTask.frontmatter.id, {
    status: 'done',
    telegram_connection_id: connection.id,
    telegram_chat_id: '12345',
    telegram_original_message_id: '100',
    telegram_accepted_message_id: '101',
    output_note: '@note/event-completed-task.md',
    completion_note: 'Event task complete.',
  })

  events.emit({
    type: EVENT_TYPES.TASK_COMPLETED,
    agent_id: 'creator-agent',
    session_id: 'creator-session',
    payload: {
      taskId: eventDone.frontmatter.id,
      title: eventDone.frontmatter.title,
      status: eventDone.frontmatter.status,
      callbackAgentId: eventDone.frontmatter.callback_agent_id,
      callbackSessionId: eventDone.frontmatter.callback_session_id,
      outputNote: eventDone.frontmatter.output_note,
    },
    timestamp: Date.now(),
  })

  await waitFor(() => sentMessages.length === 1 && messageLinks.length === 1)
  assert.equal(sentMessages[0]?.chat_id, 12345)
  assert.equal(sentMessages[0]?.reply_to_message_id, 101)
  assert.equal(sentMessages[0]?.text, 'Completion for creator-agent')
  assert.equal(messageLinks[0]?.sessionId, 'creator-session')
  assert.equal(messageLinks[0]?.itemId, 'assistant-creator-agent')

  const afterEventDelivery = (await listTasks(tasksDir)).find((task) => task.frontmatter.id === eventDone.frontmatter.id)
  assert.equal(afterEventDelivery?.frontmatter.telegram_completion_message_id, String(nextTelegramMessageId))

  const raceTask = await createTask(tasksDir, {
    id: 'acceptance-after-completion-task',
    title: 'Acceptance after completion task',
    owner: 'researcher',
    priority: 'medium',
    status: 'done',
    kind: 'background',
    outputProfile: 'generic',
    callbackAgentId: 'race-agent',
    callbackSessionId: 'race-session',
    body: 'Done before Telegram acceptance.',
    createdBy: 'agent',
  })

  await bridge.attachAcceptance({
    callbackSessionId: raceTask.frontmatter.callback_session_id!,
    connectionId: connection.id,
    chatId: 777,
    originalMessageId: 200,
    acceptedMessageId: 201,
  })

  await waitFor(() => sentMessages.length === 2)
  assert.equal(sentMessages[1]?.chat_id, 777)
  assert.equal(sentMessages[1]?.reply_to_message_id, 201)
  assert.equal(sentMessages[1]?.text, 'Completion for race-agent')

  const blockedTask = await createTask(tasksDir, {
    id: 'no-anchor-task',
    title: 'No anchor task',
    owner: 'researcher',
    priority: 'medium',
    status: 'done',
    kind: 'background',
    outputProfile: 'generic',
    callbackAgentId: 'no-anchor-agent',
    callbackSessionId: 'no-anchor-session',
    body: 'No accepted message anchor.',
    createdBy: 'agent',
  })
  events.emit({
    type: EVENT_TYPES.TASK_COMPLETED,
    agent_id: 'no-anchor-agent',
    session_id: 'no-anchor-session',
    payload: {
      taskId: blockedTask.frontmatter.id,
      title: blockedTask.frontmatter.title,
      status: blockedTask.frontmatter.status,
    },
    timestamp: Date.now(),
  })
  await new Promise((resolve) => setTimeout(resolve, 50))
  assert.equal(sentMessages.length, 2)

  bridge.stop()
  console.log('Telegram task bridge tests passed')
} finally {
  globalThis.fetch = originalFetch
  await fs.rm(tmpDir, { recursive: true, force: true })
}

async function waitFor(predicate: () => boolean): Promise<void> {
  const deadline = Date.now() + 1000
  while (Date.now() < deadline) {
    if (predicate()) return
    await new Promise((resolve) => setTimeout(resolve, 10))
  }
  throw new Error('Timed out waiting for predicate')
}
