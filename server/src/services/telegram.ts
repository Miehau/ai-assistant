import { randomBytes, randomUUID } from 'node:crypto'
import type { RuntimeContext } from '../lib/runtime.js'
import { splitModelId } from '../lib/model.js'
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
  voice?: TelegramVoice
  from?: TelegramUser
  chat: TelegramChat
  reply_to_message?: {
    message_id: number
  }
}

interface TelegramVoice {
  file_id: string
  file_unique_id: string
  duration: number
  mime_type?: string
  file_size?: number
}

export interface TelegramUpdate {
  update_id: number
  message?: TelegramMessage
}

interface TelegramCommand {
  content: string
  startsNewSession: boolean
  commandOnly: boolean
}

interface TelegramFile {
  file_id: string
  file_unique_id: string
  file_size?: number
  file_path?: string
}

const MAX_TELEGRAM_VOICE_BYTES = 20 * 1024 * 1024

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

    const rawContent = await this.extractMessageContent(connection, message)
    if (!rawContent) {
      if (message.voice) {
        return { ok: true, status: 'ignored', reason: 'voice_transcription_failed' }
      }
      await this.sendBotMessage(connection, message.chat.id, 'Only text messages are supported right now.', message.message_id)
      return { ok: true, status: 'ignored', reason: 'unsupported_message_type' }
    }

    const command = this.parseCommand(rawContent)
    if (command.commandOnly) {
      const session = await this.createTelegramSession(connection, 'Telegram session')
      await this.createMessageLink(
        connection.id,
        message.chat.id,
        message.message_id,
        session.id,
        null,
        'user',
      )
      const note = await this.sendBotMessage(
        connection,
        message.chat.id,
        'New Telegram session started. Send the next message to continue it.',
        message.message_id,
      )
      if (note != null) {
        await this.createMessageLink(connection.id, message.chat.id, note, session.id, null, 'bot')
      }
      return {
        ok: true,
        status: 'processed',
        sessionId: session.id,
        forked: false,
      }
    }

    const content = command.content
    const resolution = await this.resolveSession(connection, message, content, command.startsNewSession)
    const directReply = this.formatDirectTelegramReply(content)
    if (directReply) {
      await this.createMessageLink(
        connection.id,
        message.chat.id,
        message.message_id,
        resolution.sessionId,
        null,
        'user',
      )
      const note = await this.sendBotMessage(connection, message.chat.id, directReply, message.message_id)
      if (note != null) {
        await this.createMessageLink(connection.id, message.chat.id, note, resolution.sessionId, null, 'bot')
      }
      return {
        ok: true,
        status: 'processed',
        sessionId: resolution.sessionId,
        forked: resolution.forked,
      }
    }

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
        'That thread is still running. Try again after it finishes, or send /new to start a new session.',
        message.message_id,
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
        const outboundMessageId = await this.sendBotMessageWithRetry(connection, chatId, responseText, originalMessageId)
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
        await this.sendBotMessageWithRetry(connection, chatId, this.formatTelegramFailure(message), originalMessageId)
      } finally {
        this.runtime.agentAbortControllers.delete(agentId)
      }
    })()
  }

  /**
   * Choose which app session a Telegram message belongs to.
   *
   * /new starts a fresh Telegram session. Ordinary non-reply messages continue
   * the current chat session, while replies explicitly select the session linked
   * to the replied-to Telegram message.
   */
  private async resolveSession(
    connection: TelegramConnectionRow,
    message: TelegramMessage,
    content: string,
    startsNewSession: boolean = false,
  ): Promise<{ sessionId: string; forked: boolean }> {
    const chatId = String(message.chat.id)
    const replyToMessageId = message.reply_to_message?.message_id

    if (startsNewSession) {
      const session = await this.createTelegramSession(connection, content)
      return { sessionId: session.id, forked: false }
    }

    if (!replyToMessageId) {
      const head = await this.getChatHeadLink(connection.id, chatId)
      if (head) {
        return { sessionId: head.sessionId, forked: false }
      }
      const session = await this.createTelegramSession(connection, content)
      return { sessionId: session.id, forked: false }
    }

    const anchor = await this.getMessageLink(connection.id, chatId, replyToMessageId)
    if (anchor) {
      return { sessionId: anchor.sessionId, forked: false }
    }

    const session = await this.createTelegramSession(connection, content)
    return { sessionId: session.id, forked: false }
  }

  /** Convert Telegram text/caption/voice input into the text command the agent sees. */
  private async extractMessageContent(connection: TelegramConnectionRow, message: TelegramMessage): Promise<string> {
    const textContent = message.text?.trim() || message.caption?.trim() || ''
    if (!message.voice) return textContent

    try {
      await this.sendChatAction(connection, message.chat.id, 'typing')
      const transcript = await this.transcribeTelegramVoice(connection, message.voice)
      if (!transcript) return textContent
      const transcriptBlock = `[Telegram voice transcript]\n${transcript}`
      return textContent ? `${textContent}\n\n${transcriptBlock}` : transcriptBlock
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err)
      await this.sendBotMessage(
        connection,
        message.chat.id,
        `I couldn't transcribe that voice message: ${this.truncateForTelegram(detail, 700)}`,
        message.message_id,
      )
      return textContent
    }
  }

  /** Download a Telegram voice file and transcribe it with the configured STT model. */
  private async transcribeTelegramVoice(connection: TelegramConnectionRow, voice: TelegramVoice): Promise<string> {
    const transcriptionModel = this.runtime.config.telegramTranscriptionModel?.trim()
    if (!transcriptionModel) {
      throw new Error('TELEGRAM_TRANSCRIPTION_MODEL is not configured')
    }

    if (voice.file_size != null && voice.file_size > MAX_TELEGRAM_VOICE_BYTES) {
      throw new Error(`voice message is too large (${voice.file_size} bytes)`)
    }

    const fileResponse = await this.callTelegram<TelegramFile>(connection, 'getFile', {
      file_id: voice.file_id,
    })
    if (!fileResponse.ok || !fileResponse.result?.file_path) {
      throw new Error(fileResponse.description ?? 'Telegram getFile failed')
    }

    if (fileResponse.result.file_size != null && fileResponse.result.file_size > MAX_TELEGRAM_VOICE_BYTES) {
      throw new Error(`voice message is too large (${fileResponse.result.file_size} bytes)`)
    }

    const audioBytes = await this.downloadTelegramFile(connection, fileResponse.result.file_path)
    const { provider, model } = splitModelId(transcriptionModel)
    const transcriptionProvider = this.runtime.providers.resolve(transcriptionModel)
    if (!transcriptionProvider.transcribeAudio) {
      throw new Error(`Provider "${provider}" does not support audio transcription`)
    }

    const response = await transcriptionProvider.transcribeAudio({
      model,
      input_audio: {
        data: Buffer.from(audioBytes).toString('base64'),
        format: this.inferAudioFormat(voice.mime_type, fileResponse.result.file_path),
      },
      signal: this.runtime.shutdownController.signal,
    })

    return response.text.trim()
  }

  /** Download file bytes through Telegram's file endpoint with a hard size cap. */
  private async downloadTelegramFile(connection: TelegramConnectionRow, filePath: string): Promise<ArrayBuffer> {
    const token = this.readSecret(connection.botToken)
    const response = await fetch(`https://api.telegram.org/file/bot${token}/${filePath}`)
    if (!response.ok) {
      throw new Error(`Telegram file download failed (${response.status})`)
    }

    const contentLength = response.headers.get('content-length')
    if (contentLength && Number(contentLength) > MAX_TELEGRAM_VOICE_BYTES) {
      throw new Error(`voice message is too large (${contentLength} bytes)`)
    }

    const bytes = await response.arrayBuffer()
    if (bytes.byteLength > MAX_TELEGRAM_VOICE_BYTES) {
      throw new Error(`voice message is too large (${bytes.byteLength} bytes)`)
    }
    return bytes
  }

  /** Map Telegram MIME/path metadata to OpenRouter's transcription format string. */
  private inferAudioFormat(mimeType: string | undefined, filePath: string): string {
    const normalizedMime = mimeType?.toLowerCase() ?? ''
    if (normalizedMime.includes('ogg')) return 'ogg'
    if (normalizedMime.includes('webm')) return 'webm'
    if (normalizedMime.includes('mpeg') || normalizedMime.includes('mp3')) return 'mp3'
    if (normalizedMime.includes('mp4') || normalizedMime.includes('m4a')) return 'm4a'
    if (normalizedMime.includes('aac')) return 'aac'
    if (normalizedMime.includes('flac')) return 'flac'
    if (normalizedMime.includes('wav')) return 'wav'

    const extension = filePath.toLowerCase().split('.').pop()
    if (extension === 'oga') return 'ogg'
    if (extension && ['ogg', 'webm', 'mp3', 'm4a', 'aac', 'flac', 'wav'].includes(extension)) {
      return extension
    }
    return 'ogg'
  }

  /** Parse Telegram slash commands that affect session routing. */
  private parseCommand(content: string): TelegramCommand {
    const match = content.match(/^\/new(?:@[A-Za-z0-9_]+)?(?:\s+([\s\S]*))?$/)
    if (!match) {
      return { content, startsNewSession: false, commandOnly: false }
    }

    const nextContent = match[1]?.trim() ?? ''
    return {
      content: nextContent,
      startsNewSession: true,
      commandOnly: nextContent.length === 0,
    }
  }

  /**
   * Handle tiny conversational Telegram turns without paying the full planner
   * prompt/tool-schema cost. Only exact lightweight phrases are matched so
   * task-bearing messages such as "hey can you..." still route to the agent.
   */
  private formatDirectTelegramReply(content: string): string | null {
    const normalized = content
      .toLowerCase()
      .replace(/[.!?\s]+$/g, '')
      .trim()

    if (/^(hey|hi|hello|yo|hiya|howdy)$/.test(normalized)) {
      return 'Hey. What should I work on?'
    }

    if (/^(thanks|thank you|thx)$/.test(normalized)) {
      return 'You are welcome.'
    }

    return null
  }

  /** Create an app session owned by this Telegram connection. */
  private async createTelegramSession(connection: TelegramConnectionRow, title: string) {
    return this.runtime.repositories.sessions.create({
      userId: connection.userId,
      title: title.slice(0, 100) || 'Telegram session',
      source: 'telegram',
    })
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
    const detail = this.truncateForTelegram(message, maxDetailLength)
    return `Telegram session failed: ${detail}`
  }

  private truncateForTelegram(message: string, maxLength: number): string {
    return message.length > maxLength
      ? `${message.slice(0, maxLength).trimEnd()}...`
      : message
  }

  /** Fetch the raw DB row so internal webhook secrets remain available. */
  private async getConnectionRow(id: string, userId?: string): Promise<TelegramConnectionRow | null> {
    return this.runtime.repositories.telegram.getConnection(id, userId)
  }

  /** Find the app transcript item associated with a Telegram message. */
  private async getMessageLink(connectionId: string, chatId: string, messageId: number): Promise<TelegramMessageLinkRow | null> {
    return this.runtime.repositories.telegram.getMessageLink(connectionId, chatId, messageId)
  }

  /** Return the newest Telegram message linked in a chat. */
  private async getChatHeadLink(connectionId: string, chatId: string): Promise<TelegramMessageLinkRow | null> {
    return this.runtime.repositories.telegram.getChatHeadLink(connectionId, chatId)
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
  private async sendBotMessage(
    connection: TelegramConnectionRow,
    chatId: number,
    text: string,
    replyToMessageId?: number,
  ): Promise<number | null> {
    const payload: Record<string, unknown> = {
      chat_id: chatId,
      text,
    }
    if (replyToMessageId != null) {
      payload.reply_to_message_id = replyToMessageId
      payload.allow_sending_without_reply = true
    }
    const response = await this.callTelegram<TelegramSendMessageResponse>(connection, 'sendMessage', payload)
    return response.ok && response.result ? response.result.message_id : null
  }

  /** Retry final Telegram sends because background results are otherwise easy to lose on transient API failures. */
  private async sendBotMessageWithRetry(
    connection: TelegramConnectionRow,
    chatId: number,
    text: string,
    replyToMessageId?: number,
    attempts: number = 3,
  ): Promise<number | null> {
    for (let attempt = 1; attempt <= attempts; attempt++) {
      const messageId = await this.sendBotMessage(connection, chatId, text, replyToMessageId)
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
