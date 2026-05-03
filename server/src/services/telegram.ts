import { randomBytes, randomUUID } from 'node:crypto'
import type { RuntimeContext } from '../lib/runtime.js'
import { decrypt, deriveKey, encrypt } from '../lib/crypto.js'
import { buildDeps, prepareSessionTurn } from './session-runner.js'
import { runAgent } from '../orchestrator/runner.js'
import type { Item } from '../domain/types.js'
import type { StoredTelegramConnection, StoredTelegramMessageLink } from '../repositories/types.js'

type TelegramConnectionRow = StoredTelegramConnection
type TelegramMessageLinkRow = StoredTelegramMessageLink

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

export interface TelegramConnectInput {
  botToken: string
  allowedTelegramUserId: string
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

export interface TelegramConnectResult {
  connection: TelegramConnectionRecord | null
  test: {
    ok: boolean
    username?: string
    description?: string
  }
  webhook: TelegramWebhookRegistrationResult | null
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

  /** Return all Telegram bot connections configured by a user. */
  async listConnections(userId: string): Promise<TelegramConnectionRecord[]> {
    const rows = await this.runtime.repositories.telegram.listConnections(userId)
    return rows.map((row) => this.toConnectionRecord(row))
  }

  /** Fetch one Telegram connection, optionally scoped to an owning user. */
  async getConnection(id: string, userId?: string): Promise<TelegramConnectionRecord | null> {
    const row = await this.getConnectionRow(id, userId)
    return row ? this.toConnectionRecord(row) : null
  }

  /** Create a bot connection and store Telegram secrets encrypted when possible. */
  async createConnection(userId: string, input: CreateTelegramConnectionInput): Promise<TelegramConnectionRecord> {
    const now = Date.now()
    const row: TelegramConnectionRow = {
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
    await this.runtime.repositories.telegram.createConnection(row)
    return this.toConnectionRecord(row)
  }

  /** Create, verify, and register a Telegram bot webhook in one operation. */
  async connectBot(userId: string, input: TelegramConnectInput): Promise<TelegramConnectResult> {
    const tokenCheck = await this.callTelegramWithToken<TelegramGetMeResponse>(input.botToken, 'getMe')
    const existing = await this.findReusableConnection(
      userId,
      input.allowedTelegramUserId,
      tokenCheck.result?.username ?? null,
    )

    if (!tokenCheck.ok || !tokenCheck.result) {
      const test = { ok: false, description: tokenCheck.description }
      if (existing) {
        await this.updateConnection(existing.id, userId, {
          botToken: input.botToken,
          status: 'error',
          lastError: tokenCheck.description ?? 'Telegram getMe failed',
        })
      }
      return {
        connection: existing ? await this.getConnection(existing.id, userId) : null,
        test,
        webhook: null,
      }
    }

    const connection = existing
      ? await this.updateConnection(existing.id, userId, {
        botToken: input.botToken,
        allowedTelegramUserId: input.allowedTelegramUserId,
        botUsername: tokenCheck.result.username ?? null,
        status: 'connected',
        lastError: null,
      })
      : await this.createConnection(userId, {
        botToken: input.botToken,
        allowedTelegramUserId: input.allowedTelegramUserId,
      })

    if (!connection) {
      throw new Error('Telegram connection could not be created or updated')
    }

    const connectionId = connection.id
    const test = existing
      ? { ok: true, username: tokenCheck.result.username, description: tokenCheck.description }
      : await this.testConnection(connectionId, userId)
    if (!test.ok) {
      return {
        connection: await this.getConnection(connectionId, userId) ?? connection,
        test,
        webhook: null,
      }
    }

    const webhook = await this.registerWebhook(connectionId, userId)
    return {
      connection: await this.getConnection(connectionId, userId) ?? connection,
      test,
      webhook,
    }
  }

  /** Update bot credentials, webhook metadata, or connection status. */
  async updateConnection(id: string, userId: string, input: UpdateTelegramConnectionInput): Promise<TelegramConnectionRecord | null> {
    const existing = await this.getConnectionRow(id, userId)
    if (!existing) return null

    const updates: {
      botToken?: string
      allowedTelegramUserId?: string
      webhookUrl?: string | null
      status?: string
      lastError?: string | null
      botUsername?: string | null
      updatedAt: number
    } = {
      updatedAt: Date.now(),
    }
    if (input.botToken !== undefined && input.botToken !== null) updates.botToken = this.storeSecret(input.botToken)
    if (input.allowedTelegramUserId !== undefined) updates.allowedTelegramUserId = input.allowedTelegramUserId
    if (input.webhookUrl !== undefined) updates.webhookUrl = input.webhookUrl
    if (input.status !== undefined) updates.status = input.status
    if (input.lastError !== undefined) updates.lastError = input.lastError
    if (input.botUsername !== undefined) updates.botUsername = input.botUsername

    await this.runtime.repositories.telegram.updateConnection(id, updates)

    const updated = await this.getConnectionRow(id, userId)
    return updated ? this.toConnectionRecord(updated) : null
  }

  /** Delete a connection and its Telegram-specific webhook/message bookkeeping. */
  async deleteConnection(id: string, userId: string): Promise<boolean> {
    return this.runtime.repositories.telegram.deleteConnection(id, userId)
  }

  private async findReusableConnection(
    userId: string,
    allowedTelegramUserId: string,
    botUsername: string | null,
  ): Promise<TelegramConnectionRow | null> {
    const connections = await this.runtime.repositories.telegram.listConnections(userId)
    return connections.find((connection) => (
      connection.allowedTelegramUserId === allowedTelegramUserId &&
      (!botUsername || connection.botUsername === null || connection.botUsername === botUsername)
    )) ?? null
  }

  /** Verify the stored bot token by calling Telegram getMe. */
  async testConnection(id: string, userId: string): Promise<{ ok: boolean; username?: string; description?: string }> {
    const connection = await this.getConnectionRow(id, userId)
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

  /** Register Telegram's webhook endpoint for this connection. */
  async registerWebhook(
    id: string,
    userId: string,
    webhookUrl?: string,
  ): Promise<TelegramWebhookRegistrationResult | null> {
    const connection = await this.getConnectionRow(id, userId)
    if (!connection) return null

    const url = webhookUrl ?? this.buildWebhookUrl(connection) ?? connection.webhookUrl
    if (!url) {
      throw new Error('webhookUrl is required to register Telegram webhook. Set PUBLIC_BASE_URL or pass webhookUrl explicitly.')
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

  /** Remove Telegram's webhook registration without dropping pending updates. */
  async deleteWebhook(id: string, userId: string): Promise<TelegramWebhookRegistrationResult | null> {
    const connection = await this.getConnectionRow(id, userId)
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

  /** Read Telegram webhook status for diagnostics in the app UI. */
  async getWebhookInfo(id: string, userId: string): Promise<TelegramWebhookRegistrationResult | null> {
    const connection = await this.getConnectionRow(id, userId)
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

  /**
   * Validate and ingest a Telegram webhook update.
   *
   * This method keeps the HTTP webhook fast: it records the user message,
   * sends a short acknowledgement, then starts agent execution in the background.
   * The planner remains responsible for deciding whether to answer directly,
   * use tools, delegate research, or save a note.
   */
  async processWebhook(
    connectionId: string,
    pathSecret: string,
    headerSecret: string | undefined,
    update: TelegramUpdate,
  ): Promise<TelegramWebhookProcessResult> {
    const connection = await this.getConnectionRow(connectionId)
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

    if (await this.runtime.repositories.telegram.hasProcessedUpdate(connection.id, update.update_id)) {
      return { ok: true, status: 'ignored', reason: 'duplicate_update' }
    }

    await this.runtime.repositories.telegram.createUpdateDedupe(connection.id, update.update_id)

    const content = message.text?.trim() || message.caption?.trim() || ''
    if (!content) {
      await this.sendBotMessage(connection, message.chat.id, 'Only text messages are supported right now.')
      return { ok: true, status: 'ignored', reason: 'unsupported_message_type' }
    }

    const resolution = await this.resolveSession(connection, message, content)
    const prepared = await prepareSessionTurn(this.runtime, {
      userId: connection.userId,
      sessionId: resolution.sessionId,
      agent: 'planner',
      input: content,
      instructions: 'Telegram transport: keep user-facing replies concise. For substantial research, prefer returning a durable note path over a long chat answer: delegate the research, promote the returned artifact with notes.promote when available, and include the resulting @note/... path plus a short summary.',
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

    this.runPreparedTelegramAgent(
      connection,
      message.chat.id,
      message.message_id,
      prepared.agent.id,
      prepared.sessionId,
      prepared.model,
    )

    return {
      ok: true,
      status: 'processed',
      sessionId: prepared.sessionId,
      forked: resolution.forked,
    }
  }

  /**
   * Run a prepared Telegram agent after the webhook has already returned.
   *
   * The final assistant response is sent back to Telegram and linked to the
   * session transcript so replies can continue or fork from that message.
   */
  private runPreparedTelegramAgent(
    connection: TelegramConnectionRow,
    chatId: number,
    originalMessageId: number,
    agentId: string,
    sessionId: string,
    model: string,
  ): void {
    void (async () => {
      const deps = buildDeps(this.runtime, model)
      const agentAbort = new AbortController()
      this.runtime.agentAbortControllers.set(agentId, agentAbort)

      try {
        const result = await runAgent(agentId, deps, {
          signal: AbortSignal.any([this.runtime.shutdownController.signal, agentAbort.signal]),
        })

        const assistantItems = await this.runtime.repositories.items.listByAgent(agentId)
        const lastAssistant = [...assistantItems].reverse().find(
          (item) => item.type === 'message' && item.role === 'assistant',
        )

        const responseText = this.formatTelegramCompletion(lastAssistant?.content ?? result.result ?? '', result)
        const outboundMessageId = await this.sendBotMessageWithRetry(connection, chatId, responseText)
        if (outboundMessageId != null) {
          await this.createMessageLink(
            connection.id,
            chatId,
            outboundMessageId,
            sessionId,
            lastAssistant?.id ?? null,
            'bot',
          )
          await this.runtime.telegramTaskBridge?.attachAcceptance({
            callbackSessionId: sessionId,
            connectionId: connection.id,
            chatId,
            originalMessageId,
            acceptedMessageId: outboundMessageId,
          })
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        await this.sendBotMessageWithRetry(connection, chatId, this.formatTelegramFailure(message))
      } finally {
        this.runtime.agentAbortControllers.delete(agentId)
      }
    })()
  }

  /**
   * Choose which app session a Telegram message belongs to.
   *
   * Free messages start new sessions. Replies to the current bot head continue
   * the same session. Replies to older bot messages fork the transcript at that
   * point so Telegram can branch conversations naturally.
   */
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

    const anchor = await this.getMessageLink(connection.id, chatId, replyToMessageId)
    if (!anchor) {
      const session = await this.runtime.repositories.sessions.create({
        userId: connection.userId,
        title: content.slice(0, 100),
        source: 'telegram',
      })
      return { sessionId: session.id, forked: false }
    }

    const head = await this.getSessionHeadLink(connection.id, anchor.sessionId)
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

  /** Copy a session transcript up to an anchored Telegram message and append the new branch message. */
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

  /** Return the most recent user or assistant message item for link bookkeeping. */
  private async getLastMessageItem(agentId: string, role: 'user' | 'assistant'): Promise<Item | null> {
    const items = await this.runtime.repositories.items.listByAgent(agentId)
    return [...items].reverse().find((item) => item.type === 'message' && item.role === role) ?? null
  }

  /** Convert agent completion state into a Telegram-safe message body. */
  private formatTelegramCompletion(
    responseText: string,
    result: Awaited<ReturnType<typeof runAgent>>,
  ): string {
    if (!responseText && result.status === 'waiting') {
      responseText = 'This thread needs approval in the app before it can continue.'
    }
    if (!responseText && result.error) {
      responseText = `Telegram session failed: ${result.error}`
    }
    if (!responseText) {
      responseText = 'No assistant response was produced.'
    }

    const maxLength = 3900
    return responseText.length > maxLength
      ? `${responseText.slice(0, maxLength - 40).trimEnd()}\n\n[truncated]`
      : responseText
  }

  /** Convert background execution failures into a concise Telegram-safe error message. */
  private formatTelegramFailure(message: string): string {
    const maxDetailLength = 1200
    const detail = message.length > maxDetailLength
      ? `${message.slice(0, maxDetailLength).trimEnd()}...`
      : message
    return `Telegram session failed: ${detail}`
  }

  /** Fetch the raw DB row so internal webhook secrets remain available. */
  private async getConnectionRow(id: string, userId?: string): Promise<TelegramConnectionRow | null> {
    return this.runtime.repositories.telegram.getConnection(id, userId)
  }

  /** Find the app transcript item associated with a Telegram message. */
  private async getMessageLink(connectionId: string, chatId: string, messageId: number): Promise<TelegramMessageLinkRow | null> {
    return this.runtime.repositories.telegram.getMessageLink(connectionId, chatId, messageId)
  }

  /** Return the newest Telegram message linked to a session. */
  private async getSessionHeadLink(connectionId: string, sessionId: string): Promise<TelegramMessageLinkRow | null> {
    return this.runtime.repositories.telegram.getSessionHeadLink(connectionId, sessionId)
  }

  /** Persist the mapping between a Telegram message and an app session/item. */
  private async createMessageLink(
    connectionId: string,
    chatId: number,
    telegramMessageId: number,
    sessionId: string,
    itemId: string | null,
    senderType: 'user' | 'bot',
  ): Promise<void> {
    const existing = await this.getMessageLink(connectionId, String(chatId), telegramMessageId)
    if (existing) return

    await this.runtime.repositories.telegram.createMessageLink({
      id: randomUUID(),
      connectionId,
      telegramChatId: String(chatId),
      telegramMessageId,
      sessionId,
      itemId,
      senderType,
      createdAt: Date.now(),
    })
  }

  /** Send a plain text message through the configured bot. */
  private async sendBotMessage(connection: TelegramConnectionRow, chatId: number, text: string): Promise<number | null> {
    const response = await this.callTelegram<TelegramSendMessageResponse>(connection, 'sendMessage', {
      chat_id: chatId,
      text,
    })
    return response.ok && response.result ? response.result.message_id : null
  }

  /** Retry final Telegram sends because background results are otherwise easy to lose on transient API failures. */
  private async sendBotMessageWithRetry(
    connection: TelegramConnectionRow,
    chatId: number,
    text: string,
    attempts: number = 3,
  ): Promise<number | null> {
    for (let attempt = 1; attempt <= attempts; attempt++) {
      const messageId = await this.sendBotMessage(connection, chatId, text)
      if (messageId != null) return messageId
      if (attempt < attempts) {
        await this.delay(250 * attempt)
      }
    }
    return null
  }

  /** Sleep helper used by Telegram retry paths. */
  private async delay(ms: number): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, ms))
  }

  /** Send Telegram chat actions such as "typing". */
  private async sendChatAction(connection: TelegramConnectionRow, chatId: number, action: string): Promise<void> {
    await this.callTelegram<boolean>(connection, 'sendChatAction', {
      chat_id: chatId,
      action,
    })
  }

  /** Low-level Telegram Bot API wrapper that handles token decryption and error normalization. */
  private async callTelegram<T>(
    connection: TelegramConnectionRow,
    method: string,
    payload?: Record<string, unknown>,
  ): Promise<TelegramApiResponse<T>> {
    const token = this.readSecret(connection.botToken)
    return this.callTelegramWithToken<T>(token, method, payload)
  }

  private async callTelegramWithToken<T>(
    token: string,
    method: string,
    payload?: Record<string, unknown>,
  ): Promise<TelegramApiResponse<T>> {
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

  /** Convert a raw database row into the public connection shape returned by routes. */
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

  /** Encrypt secrets when ENCRYPTION_KEY is configured. */
  private storeSecret(value: string): string {
    return this.encKey ? encrypt(value, this.encKey) : value
  }

  /** Decrypt stored secrets when ENCRYPTION_KEY is configured. */
  private readSecret(value: string): string {
    return this.encKey ? decrypt(value, this.encKey) : value
  }

  private buildWebhookUrl(connection: TelegramConnectionRow): string | null {
    const base = this.runtime.config.publicBaseUrl?.trim().replace(/\/+$/, '')
    if (!base) return null
    return `${base}/telegram/webhook/${connection.id}/${connection.webhookPathSecret}`
  }
}
