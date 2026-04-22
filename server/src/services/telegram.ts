import { randomBytes, randomUUID } from 'node:crypto'
import { and, asc, desc, eq, lte } from 'drizzle-orm'
import * as schema from '../db/schema.js'
import type { RuntimeContext } from '../lib/runtime.js'
import { decrypt, deriveKey, encrypt } from '../lib/crypto.js'
import { buildDeps, prepareSessionTurn } from './session-runner.js'
import { runAgent } from '../orchestrator/runner.js'
import type { Item } from '../domain/types.js'

type TelegramConnectionRow = typeof schema.telegramConnections.$inferSelect
type TelegramMessageLinkRow = typeof schema.telegramMessageLinks.$inferSelect

export interface TelegramConnectionRecord {
  id: string
  userId: string
  botUsername: string | null
  allowedTelegramUserId: string
  webhookUrl: string | null
  status: string
  lastError: string | null
  botTokenConfigured: boolean
  createdAt: number
  updatedAt: number
}

export interface CreateTelegramConnectionInput {
  botToken: string
  allowedTelegramUserId: string
  webhookUrl?: string | null
}

export interface UpdateTelegramConnectionInput {
  botToken?: string | null
  allowedTelegramUserId?: string
  webhookUrl?: string | null
  status?: string
  lastError?: string | null
  botUsername?: string | null
}

export interface TelegramWebhookRegistrationResult {
  ok: boolean
  description?: string
  url?: string
  hasCustomCertificate?: boolean
  pendingUpdateCount?: number
  lastErrorDate?: number
  lastErrorMessage?: string
}

export interface TelegramWebhookProcessResult {
  ok: boolean
  status: 'processed' | 'ignored' | 'rejected'
  reason?: string
  sessionId?: string
  forked?: boolean
}

interface TelegramApiResponse<T> {
  ok: boolean
  result?: T
  description?: string
}

interface TelegramGetMeResponse {
  id: number
  is_bot: boolean
  username?: string
}

interface TelegramSendMessageResponse {
  message_id: number
}

interface TelegramWebhookInfo {
  url: string
  has_custom_certificate: boolean
  pending_update_count: number
  last_error_date?: number
  last_error_message?: string
}

interface TelegramUser {
  id: number
}

interface TelegramChat {
  id: number
  type: string
}

interface TelegramMessage {
  message_id: number
  text?: string
  caption?: string
  from?: TelegramUser
  chat: TelegramChat
  reply_to_message?: {
    message_id: number
  }
}

export interface TelegramUpdate {
  update_id: number
  message?: TelegramMessage
}

export class TelegramService {
  private readonly encKey: string | null

  constructor(private readonly runtime: RuntimeContext) {
    this.encKey = runtime.config.encryptionKey ? deriveKey(runtime.config.encryptionKey) : null
  }

  async listConnections(userId: string): Promise<TelegramConnectionRecord[]> {
    const rows = this.runtime.db
      .select()
      .from(schema.telegramConnections)
      .where(eq(schema.telegramConnections.userId, userId))
      .orderBy(asc(schema.telegramConnections.createdAt))
      .all()
    return rows.map((row) => this.toConnectionRecord(row))
  }

  async getConnection(id: string, userId?: string): Promise<TelegramConnectionRecord | null> {
    const row = this.getConnectionRow(id, userId)
    return row ? this.toConnectionRecord(row) : null
  }

  async createConnection(userId: string, input: CreateTelegramConnectionInput): Promise<TelegramConnectionRecord> {
    const now = Date.now()
    const row: typeof schema.telegramConnections.$inferInsert = {
      id: randomUUID(),
      userId,
      botToken: this.storeSecret(input.botToken),
      botUsername: null,
      allowedTelegramUserId: input.allowedTelegramUserId,
      webhookPathSecret: randomBytes(16).toString('hex'),
      webhookHeaderSecret: randomBytes(24).toString('hex'),
      webhookUrl: input.webhookUrl ?? null,
      status: 'disconnected',
      lastError: null,
      createdAt: now,
      updatedAt: now,
    }
    this.runtime.db.insert(schema.telegramConnections).values(row).run()
    return this.toConnectionRecord(row as TelegramConnectionRow)
  }

  async updateConnection(id: string, userId: string, input: UpdateTelegramConnectionInput): Promise<TelegramConnectionRecord | null> {
    const existing = this.getConnectionRow(id, userId)
    if (!existing) return null

    const updates: Partial<typeof schema.telegramConnections.$inferInsert> = {
      updatedAt: Date.now(),
    }
    if (input.botToken !== undefined && input.botToken !== null) updates.botToken = this.storeSecret(input.botToken)
    if (input.allowedTelegramUserId !== undefined) updates.allowedTelegramUserId = input.allowedTelegramUserId
    if (input.webhookUrl !== undefined) updates.webhookUrl = input.webhookUrl
    if (input.status !== undefined) updates.status = input.status
    if (input.lastError !== undefined) updates.lastError = input.lastError
    if (input.botUsername !== undefined) updates.botUsername = input.botUsername

    this.runtime.db
      .update(schema.telegramConnections)
      .set(updates)
      .where(eq(schema.telegramConnections.id, id))
      .run()

    const updated = this.getConnectionRow(id, userId)
    return updated ? this.toConnectionRecord(updated) : null
  }

  async deleteConnection(id: string, userId: string): Promise<boolean> {
    const existing = this.getConnectionRow(id, userId)
    if (!existing) return false

    this.runtime.db.transaction((tx) => {
      tx.delete(schema.telegramUpdateDedupe).where(eq(schema.telegramUpdateDedupe.connectionId, id)).run()
      tx.delete(schema.telegramMessageLinks).where(eq(schema.telegramMessageLinks.connectionId, id)).run()
      tx.delete(schema.telegramConnections).where(eq(schema.telegramConnections.id, id)).run()
    })
    return true
  }

  async testConnection(id: string, userId: string): Promise<{ ok: boolean; username?: string; description?: string }> {
    const connection = this.getConnectionRow(id, userId)
    if (!connection) return { ok: false, description: 'Telegram connection not found' }

    const response = await this.callTelegram<TelegramGetMeResponse>(connection, 'getMe')
    if (!response.ok || !response.result) {
      await this.updateConnection(id, userId, {
        status: 'error',
        lastError: response.description ?? 'Telegram getMe failed',
      })
      return { ok: false, description: response.description }
    }

    await this.updateConnection(id, userId, {
      status: 'connected',
      lastError: null,
      botUsername: response.result.username ?? null,
    })
    return {
      ok: true,
      username: response.result.username,
      description: response.description,
    }
  }

  async registerWebhook(
    id: string,
    userId: string,
    webhookUrl?: string,
  ): Promise<TelegramWebhookRegistrationResult | null> {
    const connection = this.getConnectionRow(id, userId)
    if (!connection) return null

    const url = webhookUrl ?? connection.webhookUrl
    if (!url) {
      throw new Error('webhookUrl is required to register Telegram webhook')
    }

    const response = await this.callTelegram<boolean>(connection, 'setWebhook', {
      url,
      secret_token: connection.webhookHeaderSecret,
      allowed_updates: ['message'],
    })

    await this.updateConnection(id, userId, {
      webhookUrl: url,
      status: response.ok ? 'connected' : 'error',
      lastError: response.ok ? null : (response.description ?? 'setWebhook failed'),
    })

    if (!response.ok) {
      return {
        ok: false,
        description: response.description,
      }
    }

    const info = await this.getWebhookInfo(id, userId)
    return {
      ok: true,
      description: response.description,
      ...(info ?? {}),
    }
  }

  async deleteWebhook(id: string, userId: string): Promise<TelegramWebhookRegistrationResult | null> {
    const connection = this.getConnectionRow(id, userId)
    if (!connection) return null

    const response = await this.callTelegram<boolean>(connection, 'deleteWebhook', {
      drop_pending_updates: false,
    })

    await this.updateConnection(id, userId, {
      status: response.ok ? 'disconnected' : 'error',
      lastError: response.ok ? null : (response.description ?? 'deleteWebhook failed'),
    })

    return {
      ok: response.ok,
      description: response.description,
    }
  }

  async getWebhookInfo(id: string, userId: string): Promise<TelegramWebhookRegistrationResult | null> {
    const connection = this.getConnectionRow(id, userId)
    if (!connection) return null

    const response = await this.callTelegram<TelegramWebhookInfo>(connection, 'getWebhookInfo')
    if (!response.ok || !response.result) {
      return {
        ok: false,
        description: response.description,
      }
    }

    return {
      ok: true,
      url: response.result.url,
      hasCustomCertificate: response.result.has_custom_certificate,
      pendingUpdateCount: response.result.pending_update_count,
      lastErrorDate: response.result.last_error_date,
      lastErrorMessage: response.result.last_error_message,
    }
  }

  async processWebhook(
    connectionId: string,
    pathSecret: string,
    headerSecret: string | undefined,
    update: TelegramUpdate,
  ): Promise<TelegramWebhookProcessResult> {
    const connection = this.getConnectionRow(connectionId)
    if (!connection) {
      return { ok: false, status: 'rejected', reason: 'connection_not_found' }
    }
    if (connection.webhookPathSecret !== pathSecret) {
      return { ok: false, status: 'rejected', reason: 'invalid_path_secret' }
    }
    if (connection.webhookHeaderSecret !== (headerSecret ?? '')) {
      return { ok: false, status: 'rejected', reason: 'invalid_header_secret' }
    }

    const message = update.message
    if (!message) {
      return { ok: true, status: 'ignored', reason: 'unsupported_update' }
    }
    if (message.chat.type !== 'private') {
      return { ok: true, status: 'ignored', reason: 'non_private_chat' }
    }
    if (String(message.from?.id ?? '') !== connection.allowedTelegramUserId) {
      return { ok: false, status: 'rejected', reason: 'unauthorized_user' }
    }

    const alreadyProcessed = this.runtime.db
      .select()
      .from(schema.telegramUpdateDedupe)
      .where(and(
        eq(schema.telegramUpdateDedupe.connectionId, connection.id),
        eq(schema.telegramUpdateDedupe.telegramUpdateId, update.update_id),
      ))
      .limit(1)
      .all()[0]
    if (alreadyProcessed) {
      return { ok: true, status: 'ignored', reason: 'duplicate_update' }
    }

    this.runtime.db.insert(schema.telegramUpdateDedupe).values({
      connectionId: connection.id,
      telegramUpdateId: update.update_id,
      createdAt: Date.now(),
    }).run()

    const content = message.text?.trim() || message.caption?.trim() || ''
    if (!content) {
      await this.sendBotMessage(connection, message.chat.id, 'Only text messages are supported right now.')
      return { ok: true, status: 'ignored', reason: 'unsupported_message_type' }
    }

    const resolution = await this.resolveSession(connection, message, content)
    const allowedTools = this.getTelegramAllowedTools()
    const prepared = await prepareSessionTurn(this.runtime, {
      userId: connection.userId,
      sessionId: resolution.sessionId,
      model: this.getTelegramModel(),
      input: content,
      allowedTools,
    })

    if (prepared.status === 'active') {
      const note = await this.sendBotMessage(
        connection,
        message.chat.id,
        'That thread is still running. Reply after it finishes, or send a free message to start a new session.',
      )
      if (note != null) {
        await this.createMessageLink(connection.id, message.chat.id, note, prepared.sessionId, null, 'bot')
      }
      return {
        ok: true,
        status: 'processed',
        reason: 'session_active',
        sessionId: prepared.sessionId,
        forked: resolution.forked,
      }
    }

    const inboundUserItem = await this.getLastMessageItem(prepared.agent.id, 'user')
    await this.createMessageLink(
      connection.id,
      message.chat.id,
      message.message_id,
      prepared.sessionId,
      inboundUserItem?.id ?? null,
      'user',
    )

    await this.sendChatAction(connection, message.chat.id, 'typing')

    const deps = buildDeps(this.runtime, prepared.model)
    const agentAbort = new AbortController()
    this.runtime.agentAbortControllers.set(prepared.agent.id, agentAbort)
    const result = await runAgent(prepared.agent.id, deps, {
      signal: AbortSignal.any([this.runtime.shutdownController.signal, agentAbort.signal]),
    })
    this.runtime.agentAbortControllers.delete(prepared.agent.id)

    const assistantItems = await this.runtime.repositories.items.listByAgent(prepared.agent.id)
    const lastAssistant = [...assistantItems].reverse().find(
      (item) => item.type === 'message' && item.role === 'assistant',
    )

    let responseText = lastAssistant?.content ?? result.result ?? ''
    if (!responseText && result.status === 'waiting') {
      responseText = 'This thread needs approval in the app before it can continue.'
    }
    if (!responseText && result.error) {
      responseText = `Telegram session failed: ${result.error}`
    }
    if (!responseText) {
      responseText = 'No assistant response was produced.'
    }

    const outboundMessageId = await this.sendBotMessage(connection, message.chat.id, responseText)
    if (outboundMessageId != null) {
      await this.createMessageLink(
        connection.id,
        message.chat.id,
        outboundMessageId,
        prepared.sessionId,
        lastAssistant?.id ?? null,
        'bot',
      )
    }

    return {
      ok: true,
      status: 'processed',
      sessionId: prepared.sessionId,
      forked: resolution.forked,
    }
  }

  private async resolveSession(
    connection: TelegramConnectionRow,
    message: TelegramMessage,
    content: string,
  ): Promise<{ sessionId: string; forked: boolean }> {
    const chatId = String(message.chat.id)
    const replyToMessageId = message.reply_to_message?.message_id

    if (!replyToMessageId) {
      const session = await this.runtime.repositories.sessions.create({
        userId: connection.userId,
        title: content.slice(0, 100),
        source: 'telegram',
      })
      return { sessionId: session.id, forked: false }
    }

    const anchor = this.getMessageLink(connection.id, chatId, replyToMessageId)
    if (!anchor) {
      const session = await this.runtime.repositories.sessions.create({
        userId: connection.userId,
        title: content.slice(0, 100),
        source: 'telegram',
      })
      return { sessionId: session.id, forked: false }
    }

    const head = this.getSessionHeadLink(connection.id, anchor.sessionId)
    if (!head || head.telegramMessageId === anchor.telegramMessageId) {
      return { sessionId: anchor.sessionId, forked: false }
    }

    if (!anchor.itemId) {
      const session = await this.runtime.repositories.sessions.create({
        userId: connection.userId,
        title: content.slice(0, 100),
        source: 'telegram',
      })
      return { sessionId: session.id, forked: false }
    }

    const sessionId = await this.forkSessionFromAnchor(anchor, content)
    return {
      sessionId,
      forked: true,
    }
  }

  private async forkSessionFromAnchor(anchor: TelegramMessageLinkRow, content: string): Promise<string> {
    const sourceSession = await this.runtime.repositories.sessions.getById(anchor.sessionId)
    if (!sourceSession) {
      throw new Error(`Source session not found for Telegram fork: ${anchor.sessionId}`)
    }
    const sourceRootAgent = await this.runtime.repositories.agents.findRootAgent(anchor.sessionId)
    if (!sourceRootAgent) {
      throw new Error(`Root agent not found for Telegram fork: ${anchor.sessionId}`)
    }
    const anchorItem = await this.runtime.repositories.items.getById(anchor.itemId!)
    if (!anchorItem) {
      throw new Error(`Anchor item not found for Telegram fork: ${anchor.itemId}`)
    }

    const forkedSession = await this.runtime.repositories.sessions.create({
      userId: sourceSession.userId,
      title: content.slice(0, 100) || sourceSession.title,
      parentSessionId: sourceSession.id,
      forkedFromItemId: anchorItem.id,
      source: 'telegram',
    })
    const forkedRootAgent = await this.runtime.repositories.agents.create({
      sessionId: forkedSession.id,
      task: sourceRootAgent.task,
      config: sourceRootAgent.config,
    })
    await this.runtime.repositories.sessions.update(forkedSession.id, {
      rootAgentId: forkedRootAgent.id,
    })

    const sourceItems = await this.runtime.repositories.items.listByAgent(sourceRootAgent.id)
    const copiedItems = sourceItems.filter((item) => item.sequence <= anchorItem.sequence)
    for (const item of copiedItems) {
      await this.runtime.repositories.items.create({
        agentId: forkedRootAgent.id,
        type: item.type,
        role: item.role,
        content: item.content,
        callId: item.callId,
        name: item.name,
        arguments: item.arguments,
        output: item.output,
        contentBlocks: item.contentBlocks,
        isError: item.isError,
        saveOutput: item.saveOutput,
        turnNumber: item.turnNumber,
        durationMs: item.durationMs,
      })
    }
    const lastTurn = copiedItems.reduce((maxTurn, item) => Math.max(maxTurn, item.turnNumber), 0)
    await this.runtime.repositories.agents.update(forkedRootAgent.id, {
      turnCount: lastTurn,
    })

    return forkedSession.id
  }

  private getTelegramAllowedTools(): string[] {
    return this.runtime.tools.listMetadata()
      .filter((tool) => !tool.requires_approval)
      .filter((tool) => !tool.orchestrator_intercept)
      .filter((tool) => !tool.name.startsWith('tasks.'))
      .filter((tool) => !tool.name.startsWith('preferences.'))
      .map((tool) => tool.name)
  }

  private getTelegramModel(): string {
    const defaultProvider = this.runtime.config.defaultModel.split(':', 1)[0]
    if (this.runtime.providers.list().includes(defaultProvider)) {
      return this.runtime.config.defaultModel
    }

    const fallbackProvider = this.runtime.providers.list()[0]
    if (!fallbackProvider) {
      throw new Error('No LLM providers are registered for Telegram processing')
    }
    return `${fallbackProvider}:telegram`
  }

  private async getLastMessageItem(agentId: string, role: 'user' | 'assistant'): Promise<Item | null> {
    const items = await this.runtime.repositories.items.listByAgent(agentId)
    return [...items].reverse().find((item) => item.type === 'message' && item.role === role) ?? null
  }

  private getConnectionRow(id: string, userId?: string): TelegramConnectionRow | null {
    const where = userId
      ? and(eq(schema.telegramConnections.id, id), eq(schema.telegramConnections.userId, userId))
      : eq(schema.telegramConnections.id, id)
    const row = this.runtime.db
      .select()
      .from(schema.telegramConnections)
      .where(where)
      .limit(1)
      .all()[0]
    return row ?? null
  }

  private getMessageLink(connectionId: string, chatId: string, messageId: number): TelegramMessageLinkRow | null {
    const row = this.runtime.db
      .select()
      .from(schema.telegramMessageLinks)
      .where(and(
        eq(schema.telegramMessageLinks.connectionId, connectionId),
        eq(schema.telegramMessageLinks.telegramChatId, chatId),
        eq(schema.telegramMessageLinks.telegramMessageId, messageId),
      ))
      .limit(1)
      .all()[0]
    return row ?? null
  }

  private getSessionHeadLink(connectionId: string, sessionId: string): TelegramMessageLinkRow | null {
    const row = this.runtime.db
      .select()
      .from(schema.telegramMessageLinks)
      .where(and(
        eq(schema.telegramMessageLinks.connectionId, connectionId),
        eq(schema.telegramMessageLinks.sessionId, sessionId),
      ))
      .orderBy(desc(schema.telegramMessageLinks.createdAt), desc(schema.telegramMessageLinks.telegramMessageId))
      .limit(1)
      .all()[0]
    return row ?? null
  }

  private async createMessageLink(
    connectionId: string,
    chatId: number,
    telegramMessageId: number,
    sessionId: string,
    itemId: string | null,
    senderType: 'user' | 'bot',
  ): Promise<void> {
    const existing = this.getMessageLink(connectionId, String(chatId), telegramMessageId)
    if (existing) return

    this.runtime.db.insert(schema.telegramMessageLinks).values({
      id: randomUUID(),
      connectionId,
      telegramChatId: String(chatId),
      telegramMessageId,
      sessionId,
      itemId,
      senderType,
      createdAt: Date.now(),
    }).run()
  }

  private async sendBotMessage(connection: TelegramConnectionRow, chatId: number, text: string): Promise<number | null> {
    const response = await this.callTelegram<TelegramSendMessageResponse>(connection, 'sendMessage', {
      chat_id: chatId,
      text,
    })
    return response.ok && response.result ? response.result.message_id : null
  }

  private async sendChatAction(connection: TelegramConnectionRow, chatId: number, action: string): Promise<void> {
    await this.callTelegram<boolean>(connection, 'sendChatAction', {
      chat_id: chatId,
      action,
    })
  }

  private async callTelegram<T>(
    connection: TelegramConnectionRow,
    method: string,
    payload?: Record<string, unknown>,
  ): Promise<TelegramApiResponse<T>> {
    const token = this.readSecret(connection.botToken)
    const response = await fetch(`https://api.telegram.org/bot${token}/${method}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: payload ? JSON.stringify(payload) : '{}',
    })

    const json = await response.json() as TelegramApiResponse<T>
    if (!response.ok) {
      return {
        ok: false,
        description: json.description ?? `Telegram API request failed (${response.status})`,
      }
    }

    return json
  }

  private toConnectionRecord(row: TelegramConnectionRow): TelegramConnectionRecord {
    return {
      id: row.id,
      userId: row.userId,
      botUsername: row.botUsername ?? null,
      allowedTelegramUserId: row.allowedTelegramUserId,
      webhookUrl: row.webhookUrl ?? null,
      status: row.status,
      lastError: row.lastError ?? null,
      botTokenConfigured: Boolean(row.botToken),
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    }
  }

  private storeSecret(value: string): string {
    return this.encKey ? encrypt(value, this.encKey) : value
  }

  private readSecret(value: string): string {
    return this.encKey ? decrypt(value, this.encKey) : value
  }
}
