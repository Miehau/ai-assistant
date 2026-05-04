import { createHash } from 'crypto'
import fs from 'fs/promises'
import os from 'os'
import path from 'path'

import { and, eq } from 'drizzle-orm'
import * as schema from '../db/schema.js'
import { loadConfig } from '../lib/config.js'
import { initRuntime, type RuntimeContext } from '../lib/runtime.js'
import { TelegramService } from '../services/telegram.js'

async function main() {
  console.log('=== E2E Test Suite ===\n')
  let passed = 0
  let failed = 0

  function assert(condition: boolean, name: string) {
    if (condition) {
      console.log(`  ✓ ${name}`)
      passed++
    } else {
      console.log(`  ✗ ${name}`)
      failed++
    }
  }

  // Use a temporary database file for test isolation
  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'e2e-test-'))
  const dbPath = path.join(tmpDir, 'test.db')
  process.env.DATABASE_URL = dbPath
  process.env.TASKS_DIR = path.join(tmpDir, 'tasks')
  process.env.WORKSPACE_DIR = path.join(tmpDir, 'workspace')
  process.env.SESSION_FILES_DIR = path.join(tmpDir, 'sessions')
  process.env.ENCRYPTION_KEY = 'test-encryption-key'
  process.env.TELEGRAM_TRANSCRIPTION_MODEL = 'openrouter:openai/whisper-1'

  // Clear provider keys so none are registered
  delete process.env.ANTHROPIC_API_KEY
  delete process.env.OPENAI_API_KEY
  delete process.env.OLLAMA_BASE_URL
  delete process.env.OPENROUTER_API_KEY

  let runtime: RuntimeContext
  const originalFetch = globalThis.fetch

  try {
    // ──────────────────────────────────────────
    // 1. Initialize runtime
    // ──────────────────────────────────────────
    console.log('1. Runtime initialization')
    const config = loadConfig()
    assert(config.databaseUrl === dbPath, 'Config picks up DATABASE_URL')

    runtime = await initRuntime(config)
    assert(runtime != null, 'Runtime initializes without error')

    const { repositories: repos, providers, tools, events } = runtime

    // ──────────────────────────────────────────
    // 2. UserRepository — create + look up a test user
    // ──────────────────────────────────────────
    console.log('\n2. UserRepository')
    const testKeyHash = createHash('sha256').update('e2e-test-key').digest('hex')
    const devUser = await repos.users.create({ email: 'e2e@test.local', apiKeyHash: testKeyHash })
    assert(devUser != null, 'Test user created')
    assert(devUser.email === 'e2e@test.local', 'Test user email matches')
    assert(devUser.apiKeyHash === testKeyHash, 'Test user apiKeyHash matches')

    const byId = await repos.users.getById(devUser.id)
    assert(byId != null && byId.id === devUser.id, 'getById returns same user')

    const byHash = await repos.users.getByApiKeyHash(testKeyHash)
    assert(byHash != null && byHash.id === devUser.id, 'getByApiKeyHash returns same user')

    const missing = await repos.users.getByApiKeyHash('nonexistent-hash')
    assert(missing === null, 'getByApiKeyHash returns null for unknown hash')

    // ──────────────────────────────────────────
    // 3. SessionRepository — CRUD
    // ──────────────────────────────────────────
    console.log('\n3. SessionRepository')
    const session = await repos.sessions.create({ userId: devUser!.id, title: 'Test Session' })
    assert(session.id.length > 0, 'Session created with an id')
    assert(session.title === 'Test Session', 'Session title matches')
    assert(session.status === 'active', 'Session status defaults to active')

    const fetchedSession = await repos.sessions.getById(session.id)
    assert(fetchedSession != null && fetchedSession.id === session.id, 'getById returns created session')

    const sessions = await repos.sessions.listByUser(devUser!.id)
    assert(sessions.length >= 1, 'listByUser returns at least 1 session')
    assert(sessions.some((s) => s.id === session.id), 'listByUser includes created session')

    const updatedSession = await repos.sessions.update(session.id, { title: 'Updated Title', status: 'archived' })
    assert(updatedSession.title === 'Updated Title', 'Session title updated')
    assert(updatedSession.status === 'archived', 'Session status updated to archived')

    // Create a second session to test delete isolation
    const session2 = await repos.sessions.create({ userId: devUser!.id, title: 'To Delete' })
    await repos.sessions.delete(session2.id)
    const deleted = await repos.sessions.getById(session2.id)
    assert(deleted === null, 'Deleted session is no longer found')

    // ──────────────────────────────────────────
    // 4. AgentRepository — CRUD
    // ──────────────────────────────────────────
    console.log('\n4. AgentRepository')
    const agentConfig = {
      model: 'test:model',
      provider: 'test',
      max_turns: 10,
      max_tool_calls_per_step: 5,
      tool_execution_timeout_ms: 30000,
    }
    const agent = await repos.agents.create({
      sessionId: session.id,
      task: 'Test task',
      config: agentConfig,
    })
    assert(agent.id.length > 0, 'Agent created with an id')
    assert(agent.status === 'pending', 'Agent status defaults to pending')
    assert(agent.task === 'Test task', 'Agent task matches')
    assert(agent.config.model === 'test:model', 'Agent config model matches')

    const fetchedAgent = await repos.agents.getById(agent.id)
    assert(fetchedAgent != null && fetchedAgent.id === agent.id, 'getById returns created agent')

    const updatedAgent = await repos.agents.update(agent.id, {
      status: 'running',
      turnCount: 3,
      result: 'some result',
    })
    assert(updatedAgent.status === 'running', 'Agent status updated to running')
    assert(updatedAgent.turnCount === 3, 'Agent turnCount updated')
    assert(updatedAgent.result === 'some result', 'Agent result updated')

    const agentsBySession = await repos.agents.listBySession(session.id)
    assert(agentsBySession.length >= 1, 'listBySession returns at least 1 agent')
    assert(agentsBySession.some((a) => a.id === agent.id), 'listBySession includes created agent')

    // Test child agent
    const childAgent = await repos.agents.create({
      sessionId: session.id,
      parentId: agent.id,
      task: 'Child task',
      config: agentConfig,
      depth: 1,
    })
    const children = await repos.agents.listByParent(agent.id)
    assert(children.length === 1, 'listByParent returns 1 child')
    assert(children[0].id === childAgent.id, 'listByParent returns correct child')

    // ──────────────────────────────────────────
    // 5. ItemRepository — create, listByAgent
    // ──────────────────────────────────────────
    console.log('\n5. ItemRepository')
    const item1 = await repos.items.create({
      agentId: agent.id,
      type: 'message',
      role: 'user',
      content: 'Hello agent',
      turnNumber: 1,
    })
    assert(item1.id.length > 0, 'Item created with an id')
    assert(item1.type === 'message', 'Item type is message')
    assert(item1.content === 'Hello agent', 'Item content matches')
    assert(item1.sequence === 0, 'First item sequence is 0')

    const item2 = await repos.items.create({
      agentId: agent.id,
      type: 'function_call',
      callId: 'call-123',
      name: 'read_file',
      arguments: '{"path":"/tmp/test"}',
      turnNumber: 1,
    })
    assert(item2.sequence === 1, 'Second item sequence is 1')

    const item3 = await repos.items.create({
      agentId: agent.id,
      type: 'function_call_output',
      callId: 'call-123',
      output: 'file contents here',
      turnNumber: 1,
    })

    const items = await repos.items.listByAgent(agent.id)
    assert(items.length === 3, 'listByAgent returns 3 items')
    assert(items[0].sequence < items[1].sequence, 'Items are ordered by sequence')

    const outputItem = await repos.items.getOutputByCallId('call-123')
    assert(outputItem != null, 'getOutputByCallId finds the output item')
    assert(outputItem!.output === 'file contents here', 'Output item content matches')

    const missingOutput = await repos.items.getOutputByCallId('nonexistent')
    assert(missingOutput === null, 'getOutputByCallId returns null for unknown callId')

    // ──────────────────────────────────────────
    // 6. ToolOutputRepository — save, getById, listByAgent
    // ──────────────────────────────────────────
    console.log('\n6. ToolOutputRepository')
    const toolOutput = await repos.toolOutputs.save({
      agentId: agent.id,
      callId: 'call-456',
      toolName: 'read_file',
      data: { content: 'some file data', lines: 42 },
    })
    assert(toolOutput.id.length > 0, 'ToolOutput created with an id')
    assert(toolOutput.toolName === 'read_file', 'ToolOutput toolName matches')
    assert((toolOutput.data as any).lines === 42, 'ToolOutput data preserved')

    const fetchedTO = await repos.toolOutputs.getById(toolOutput.id)
    assert(fetchedTO != null && fetchedTO.id === toolOutput.id, 'getById returns created tool output')

    const toolOutputs = await repos.toolOutputs.listByAgent(agent.id)
    assert(toolOutputs.length >= 1, 'listByAgent returns at least 1 tool output')

    const emptyOutputs = await repos.toolOutputs.listByAgent('nonexistent-agent')
    assert(emptyOutputs.length === 0, 'listByAgent returns empty for unknown agent')

    // ──────────────────────────────────────────
    // 7. ModelRepository — create, list, delete
    // ──────────────────────────────────────────
    console.log('\n7. ModelRepository')
    const model = await repos.models.create({
      provider: 'openai',
      name: 'gpt-4o',
    })
    assert(model.id.length > 0, 'Model created with an id')
    assert(model.provider === 'openai', 'Model provider matches')
    assert(model.name === 'gpt-4o', 'Model name matches')

    const models = await repos.models.list()
    assert(models.length >= 1, 'list returns at least 1 model')

    const fetchedModel = await repos.models.getById(model.id)
    assert(fetchedModel != null && fetchedModel.id === model.id, 'getById returns created model')

    await repos.models.delete(model.id)
    const deletedModel = await repos.models.getById(model.id)
    assert(deletedModel === null, 'Deleted model is no longer found')

    // ──────────────────────────────────────────
    // 8. ApiKeyRepository — upsert, getByProvider, delete
    // ──────────────────────────────────────────
    console.log('\n8. ApiKeyRepository')
    const apiKey = await repos.apiKeys.upsert('anthropic', 'enc-key-123')
    assert(apiKey.provider === 'anthropic', 'ApiKey provider matches')
    assert(apiKey.encryptedKey === 'enc-key-123', 'ApiKey encryptedKey matches')

    const fetchedKey = await repos.apiKeys.getByProvider('anthropic')
    assert(fetchedKey != null && fetchedKey.provider === 'anthropic', 'getByProvider returns created key')

    // Upsert should update, not duplicate
    const updatedKey = await repos.apiKeys.upsert('anthropic', 'enc-key-456')
    assert(updatedKey.encryptedKey === 'enc-key-456', 'Upsert updates existing key')

    await repos.apiKeys.delete('anthropic')
    const deletedKey = await repos.apiKeys.getByProvider('anthropic')
    assert(deletedKey === null, 'Deleted api key is no longer found')

    const missingKey = await repos.apiKeys.getByProvider('nonexistent')
    assert(missingKey === null, 'getByProvider returns null for unknown provider')

    // ──────────────────────────────────────────
    // 9. SystemPromptRepository — CRUD
    // ──────────────────────────────────────────
    console.log('\n9. SystemPromptRepository')
    const prompt = await repos.systemPrompts.create({
      name: 'Default Agent',
      content: 'You are a helpful assistant.',
    })
    assert(prompt.id.length > 0, 'SystemPrompt created with an id')
    assert(prompt.name === 'Default Agent', 'SystemPrompt name matches')
    assert(prompt.content === 'You are a helpful assistant.', 'SystemPrompt content matches')

    const prompts = await repos.systemPrompts.list()
    assert(prompts.length >= 1, 'list returns at least 1 prompt')

    const fetchedPrompt = await repos.systemPrompts.getById(prompt.id)
    assert(fetchedPrompt != null && fetchedPrompt.id === prompt.id, 'getById returns created prompt')

    const updatedPrompt = await repos.systemPrompts.update(prompt.id, {
      name: 'Updated Agent',
      content: 'You are a very helpful assistant.',
    })
    assert(updatedPrompt.name === 'Updated Agent', 'SystemPrompt name updated')
    assert(updatedPrompt.content === 'You are a very helpful assistant.', 'SystemPrompt content updated')

    await repos.systemPrompts.delete(prompt.id)
    const deletedPrompt = await repos.systemPrompts.getById(prompt.id)
    assert(deletedPrompt === null, 'Deleted system prompt is no longer found')

    // ──────────────────────────────────────────
    // 10. PreferenceRepository — set, get, delete
    // ──────────────────────────────────────────
    console.log('\n10. PreferenceRepository')
    await repos.preferences.set('theme', 'dark')
    const theme = await repos.preferences.get('theme')
    assert(theme === 'dark', 'Preference get returns set value')

    // Overwrite
    await repos.preferences.set('theme', 'light')
    const updated = await repos.preferences.get('theme')
    assert(updated === 'light', 'Preference set overwrites existing value')

    await repos.preferences.delete('theme')
    const deletedPref = await repos.preferences.get('theme')
    assert(deletedPref === null, 'Deleted preference returns null')

    const missingPref = await repos.preferences.get('nonexistent-key')
    assert(missingPref === null, 'get returns null for unknown key')

    // ──────────────────────────────────────────
    // 11. Tool registry
    // ──────────────────────────────────────────
    console.log('\n11. Tool registry')
    const toolList = tools.listMetadata()
    assert(toolList.length > 0, `Tool registry has ${toolList.length} tools registered`)

    const toolNames = toolList.map((t) => t.name)
    assert(toolNames.includes('read_file') || toolNames.some((n) => n.includes('file')), 'File tools are registered')
    assert(toolNames.includes('shell') || toolNames.some((n) => n.includes('shell')), 'Shell tools are registered')

    // Verify metadata has expected shape
    const firstTool = toolList[0]
    assert(typeof firstTool.name === 'string' && firstTool.name.length > 0, 'Tool metadata has a name')
    assert(typeof firstTool.description === 'string', 'Tool metadata has a description')
    assert(firstTool.parameters != null, 'Tool metadata has parameters schema')

    // ──────────────────────────────────────────
    // 12. Provider registry (empty in test)
    // ──────────────────────────────────────────
    console.log('\n12. Provider registry')
    const providerList = providers.list()
    assert(providerList.length === 0, 'Provider registry is empty (no API keys configured)')

    let resolveError: string | null = null
    try {
      providers.resolve('openai:gpt-4o')
    } catch (err) {
      resolveError = (err as Error).message
    }
    assert(resolveError != null, 'Resolving unregistered provider throws error')
    assert(resolveError!.includes('not found'), 'Error message indicates provider not found')

    // ──────────────────────────────────────────
    // 13. Event emitter
    // ──────────────────────────────────────────
    console.log('\n13. Event emitter')
    let receivedEvent: any = null
    const testEvent = {
      type: 'agent:started' as const,
      agent_id: 'test-agent',
      session_id: 'test-session',
      payload: { task: 'test-task', model: 'test-model', parentId: null, depth: 0 },
      timestamp: Date.now(),
    }

    // Test subscribeOnce
    const eventPromise = (events as any).subscribeOnce({
      agent_id: 'test-agent',
    })
    events.emit(testEvent)
    receivedEvent = await Promise.race([
      eventPromise,
      new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 1000)),
    ])
    assert(receivedEvent != null, 'Event emitter delivers events')
    assert(receivedEvent.type === 'agent:started', 'Received event type matches')
    assert(receivedEvent.agent_id === 'test-agent', 'Received event agent_id matches')
    assert((receivedEvent.payload as any).task === 'test-task', 'Received event payload matches')

    // Test filtering - event should not match different agent_id
    let filterTimeout = false
    const filteredPromise = (events as any).subscribeOnce({
      agent_id: 'other-agent',
    })
    events.emit(testEvent) // agent_id is 'test-agent', filter is 'other-agent'
    try {
      await Promise.race([
        filteredPromise,
        new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 100)),
      ])
    } catch {
      filterTimeout = true
    }
    assert(filterTimeout, 'Event emitter filters by agent_id correctly')

    // ──────────────────────────────────────────
    // 14. Telegram integration
    // ──────────────────────────────────────────
    console.log('\n14. Telegram integration')
    let providerGenerateCalls = 0
    const stubProvider = {
      async generate() {
        providerGenerateCalls++
        return {
          content: 'Stubbed Telegram reply',
          usage: { input_tokens: 1, output_tokens: 1 },
          finish_reason: 'stop',
        }
      },
      async *stream() {
        yield {
          type: 'done' as const,
          response: {
            content: 'Stubbed Telegram reply',
            usage: { input_tokens: 1, output_tokens: 1 },
            finish_reason: 'stop',
          },
        }
      },
      async transcribeAudio() {
        return {
          text: 'voice note transcript from OpenRouter',
          usage: { input_tokens: 1, output_tokens: 4 },
        }
      },
    }
    providers.register('stub', stubProvider)
    providers.register('openrouter', stubProvider)

    let sentMessageId = 1000
    const sentMessages: Array<Record<string, unknown>> = []
    globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
      const url = typeof input === 'string'
        ? input
        : input instanceof URL
          ? input.toString()
          : input.url
      const method = url.split('/').pop() ?? ''

      if (url.startsWith('https://api.telegram.org/file/')) {
        return new Response(Buffer.from('fake-ogg-audio'), {
          status: 200,
          headers: {
            'content-type': 'audio/ogg',
            'content-length': String(Buffer.byteLength('fake-ogg-audio')),
          },
        })
      }

      if (method === 'sendChatAction') {
        return new Response(JSON.stringify({ ok: true, result: true }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
      }

      if (method === 'sendMessage') {
        sentMessageId += 1
        sentMessages.push(JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>)
        return new Response(JSON.stringify({
          ok: true,
          result: {
            message_id: sentMessageId,
          },
        }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
      }

      if (method === 'getMe') {
        return new Response(JSON.stringify({
          ok: true,
          result: {
            id: 123,
            is_bot: true,
            username: 'test_bot',
          },
        }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
      }

      if (method === 'getFile') {
        return new Response(JSON.stringify({
          ok: true,
          result: {
            file_id: 'voice-file',
            file_unique_id: 'voice-file-unique',
            file_size: 14,
            file_path: 'voice/file_123.ogg',
          },
        }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
      }

      if (method === 'getWebhookInfo') {
        return new Response(JSON.stringify({
          ok: true,
          result: {
            url: 'https://example.test/telegram/webhook',
            has_custom_certificate: false,
            pending_update_count: 0,
          },
        }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
      }

      if (method === 'setWebhook' || method === 'deleteWebhook') {
        return new Response(JSON.stringify({ ok: true, result: true }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
      }

      throw new Error(`Unexpected fetch call in Telegram test: ${url} ${JSON.stringify(init ?? {})}`)
    }) as typeof fetch

    const telegram = new TelegramService(runtime)
    const telegramConn = await telegram.createConnection(devUser!.id, {
      botToken: 'telegram-token',
      allowedTelegramUserId: '42',
      webhookUrl: 'https://example.test/telegram/webhook/connection',
    })
    const telegramConnRow = runtime.db
      .select()
      .from(schema.telegramConnections)
      .where(eq(schema.telegramConnections.id, telegramConn.id))
      .limit(1)
      .all()[0]
    assert(telegramConnRow.botToken !== 'telegram-token', 'Telegram bot token is stored encrypted or obfuscated')

    const testConnection = await telegram.testConnection(telegramConn.id, devUser!.id)
    assert(testConnection.ok, 'Telegram testConnection succeeds with mocked getMe')

    const sessionCountBeforeTelegram = (await repos.sessions.listByUser(devUser!.id)).length

    const greeting = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 100,
        message: {
          message_id: 100,
          text: 'hey',
          from: { id: 42 },
          chat: { id: 501, type: 'private' },
        },
      },
    )
    assert(greeting.status === 'processed' && greeting.sessionId != null, 'Telegram greeting is processed')
    assert(providerGenerateCalls === 0, 'Telegram greeting does not invoke the planner LLM')
    assert(sentMessages.some((payload) => (
      payload.reply_to_message_id === 100 &&
      payload.text === 'Hey. What should I work on?'
    )), 'Telegram greeting receives a direct bot reply')

    const invalidHeader = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      'wrong-secret',
      {
        update_id: 1,
        message: {
          message_id: 10,
          text: 'hello',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(invalidHeader.status === 'rejected', 'Telegram webhook rejects invalid header secret')

    const first = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 2,
        message: {
          message_id: 10,
          text: 'first thread',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(first.status === 'processed' && first.sessionId != null, 'Telegram free message starts a session')

    const firstSessionId = first.sessionId!
    await waitForTelegramBotReplies(runtime, firstSessionId, 1)
    assert(sentMessages.some((payload) => payload.reply_to_message_id === 10), 'Telegram bot replies to the inbound message')
    const firstSession = await repos.sessions.getById(firstSessionId)
    assert(firstSession?.source === 'telegram', 'Telegram-created session is marked with telegram source')

    const duplicate = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 2,
        message: {
          message_id: 10,
          text: 'first thread',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(duplicate.status === 'ignored' && duplicate.reason === 'duplicate_update', 'Telegram dedupes repeated update ids')

    const headAfterFirst = runtime.db
      .select()
      .from(schema.telegramMessageLinks)
      .where(eq(schema.telegramMessageLinks.sessionId, firstSessionId))
      .all()
      .filter((link) => link.senderType === 'bot' && link.itemId != null)
      .sort((a, b) => (a.createdAt - b.createdAt) || (a.telegramMessageId - b.telegramMessageId))
    const firstBotReplyId = headAfterFirst[headAfterFirst.length - 1]?.telegramMessageId
    assert(typeof firstBotReplyId === 'number', 'Telegram stores outbound bot message link for first session')

    const second = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 3,
        message: {
          message_id: 11,
          text: 'continue same thread',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(second.sessionId === firstSessionId && !second.forked, 'Telegram free message continues the current chat session')
    await waitForTelegramBotReplies(runtime, firstSessionId, 2)
    assert(sentMessages.some((payload) => payload.reply_to_message_id === 11), 'Telegram continuation reply is anchored to the inbound message')

    const secondSessionLinks = runtime.db
      .select()
      .from(schema.telegramMessageLinks)
      .where(eq(schema.telegramMessageLinks.sessionId, firstSessionId))
      .all()
      .sort((a, b) => a.createdAt - b.createdAt)
    const secondBotReplyId = secondSessionLinks[secondSessionLinks.length - 1]?.telegramMessageId

    const newFromReply = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 4,
        message: {
          message_id: 12,
          text: '/new separate topic',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
          reply_to_message: { message_id: firstBotReplyId! },
        },
      },
    )
    assert(newFromReply.status === 'processed' && newFromReply.sessionId != null, '/new with text is processed')
    assert(newFromReply.sessionId !== firstSessionId && !newFromReply.forked, '/new with text starts a new session even when sent as a reply')
    await waitForTelegramBotReplies(runtime, newFromReply.sessionId!, 1)
    assert(sentMessages.some((payload) => payload.reply_to_message_id === 12), '/new with text replies to the command message')

    const continuedNew = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 5,
        message: {
          message_id: 13,
          text: 'continue separate topic without replying',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(continuedNew.sessionId === newFromReply.sessionId && !continuedNew.forked, 'Telegram free message after /new continues that new session')
    await waitForTelegramBotReplies(runtime, newFromReply.sessionId!, 2)
    assert(sentMessages.some((payload) => payload.reply_to_message_id === 13), 'Telegram reply after /new continuation is anchored to the inbound message')

    const commandOnly = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 6,
        message: {
          message_id: 14,
          text: '/new',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(commandOnly.status === 'processed' && commandOnly.sessionId != null, '/new command-only creates a Telegram session')
    assert(commandOnly.sessionId !== firstSessionId && !commandOnly.forked, '/new command-only advances the current chat session')
    assert(sentMessages.some((payload) => (
      payload.reply_to_message_id === 14 &&
      payload.text === 'New Telegram session started. Send the next message to continue it.'
    )), '/new command-only acknowledgement is anchored to the command message')

    const commandOnlyLinks = runtime.db
      .select()
      .from(schema.telegramMessageLinks)
      .where(eq(schema.telegramMessageLinks.sessionId, commandOnly.sessionId!))
      .all()
      .sort((a, b) => (a.createdAt - b.createdAt) || (a.telegramMessageId - b.telegramMessageId))
    const commandOnlyAnchorId = commandOnlyLinks.find((link) => link.senderType === 'bot')?.telegramMessageId
    assert(typeof commandOnlyAnchorId === 'number', '/new command-only stores a bot anchor message')

    const resumedNew = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 7,
        message: {
          message_id: 15,
          text: 'first prompt after command-only new',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(resumedNew.sessionId === commandOnly.sessionId && !resumedNew.forked, 'Free message after /new command-only continues that new session')
    await waitForTelegramBotReplies(runtime, commandOnly.sessionId!, 2)
    assert(sentMessages.some((payload) => payload.reply_to_message_id === 15), 'Telegram reply after /new command-only is anchored to the inbound message')

    const resumedFirst = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 8,
        message: {
          message_id: 16,
          text: 'back to first session',
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
          reply_to_message: { message_id: firstBotReplyId! },
        },
      },
    )
    assert(resumedFirst.sessionId === firstSessionId && !resumedFirst.forked, 'Replying to an earlier message brings that Telegram session back into scope')
    await waitForTelegramBotReplies(runtime, firstSessionId, 3)
    assert(sentMessages.some((payload) => payload.reply_to_message_id === 16), 'Telegram resumed-session reply is anchored to the inbound reply')

    const voice = await telegram.processWebhook(
      telegramConn.id,
      telegramConnRow.webhookPathSecret,
      telegramConnRow.webhookHeaderSecret,
      {
        update_id: 9,
        message: {
          message_id: 17,
          voice: {
            file_id: 'voice-file',
            file_unique_id: 'voice-file-unique',
            duration: 3,
            mime_type: 'audio/ogg',
            file_size: 14,
          },
          from: { id: 42 },
          chat: { id: 500, type: 'private' },
        },
      },
    )
    assert(voice.sessionId === firstSessionId && !voice.forked, 'Telegram voice message is transcribed and continues the active session')
    await waitForTelegramBotReplies(runtime, firstSessionId, 4)
    assert(sentMessages.some((payload) => payload.reply_to_message_id === 17), 'Telegram voice reply is anchored to the inbound message')

    const firstSessionAgents = await repos.agents.listBySession(firstSessionId)
    const firstRootAgent = firstSessionAgents.find((agent) => agent.parentId === null)
    const firstRootItems = firstRootAgent ? await repos.items.listByAgent(firstRootAgent.id) : []
    assert(
      firstRootItems.some((item) => item.role === 'user' && item.content?.includes('voice note transcript from OpenRouter')),
      'Telegram voice transcript is persisted as the user turn text',
    )

    const sessionCountAfterTelegram = (await repos.sessions.listByUser(devUser!.id)).length
    assert(sessionCountAfterTelegram === sessionCountBeforeTelegram + 4, 'Telegram tests created one direct greeting session, one initial session, and two /new sessions')
    assert(secondBotReplyId !== firstBotReplyId, 'Continuing the Telegram thread advances the head message id')

    // ──────────────────────────────────────────
    // 15. Cleanup — delete test session (cascades agents, items, tool outputs)
    // ──────────────────────────────────────────
    console.log('\n15. Cleanup')
    await repos.sessions.delete(session.id)
    const cleanedSession = await repos.sessions.getById(session.id)
    assert(cleanedSession === null, 'Test session cleaned up')

    const cleanedAgents = await repos.agents.listBySession(session.id)
    assert(cleanedAgents.length === 0, 'Agents cleaned up after session delete')

    const cleanedItems = await repos.items.listByAgent(agent.id)
    assert(cleanedItems.length === 0, 'Items cleaned up after session delete')

    const cleanedToolOutputs = await repos.toolOutputs.listByAgent(agent.id)
    assert(cleanedToolOutputs.length === 0, 'Tool outputs cleaned up after session delete')

  } catch (err) {
    console.error('\nFATAL ERROR:', err)
    failed++
  } finally {
    globalThis.fetch = originalFetch
    // Remove temp database
    await fs.rm(tmpDir, { recursive: true, force: true }).catch(() => {})
  }

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`)
  process.exit(failed > 0 ? 1 : 0)
}

async function waitForTelegramBotReplies(
  runtime: RuntimeContext,
  sessionId: string,
  expectedCount: number,
): Promise<void> {
  const deadline = Date.now() + 3000
  while (Date.now() < deadline) {
    const count = runtime.db
      .select()
      .from(schema.telegramMessageLinks)
      .where(and(
        eq(schema.telegramMessageLinks.sessionId, sessionId),
        eq(schema.telegramMessageLinks.senderType, 'bot'),
      ))
      .all()
      .length
    if (count >= expectedCount) return
    await new Promise((resolve) => setTimeout(resolve, 25))
  }
  throw new Error(`Timed out waiting for ${expectedCount} Telegram bot replies in session ${sessionId}`)
}

main()
