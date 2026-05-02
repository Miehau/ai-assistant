import { randomUUID } from 'node:crypto'
import type { RuntimeContext } from '../lib/runtime.js'
import { decrypt, deriveKey } from '../lib/crypto.js'
import { logger } from '../lib/logger.js'
import { listTasks, updateTask } from '../tasks/storage.js'
import { EVENT_TYPES } from '../events/types.js'

interface TelegramTaskBridgeOptions {
  tasksDir: string
}

export class TelegramTaskBridge {
  private running = false
  private processing = false
  private abortController: AbortController | null = null
  private readonly encKey: string | null

  constructor(
    private readonly runtime: RuntimeContext,
    private readonly options: TelegramTaskBridgeOptions,
  ) {
    this.encKey = runtime.config.encryptionKey ? deriveKey(runtime.config.encryptionKey) : null
  }

  start(): void {
    if (this.running) return
    this.running = true
    this.abortController = new AbortController()
    void this.reconcilePendingCompletions()
    void this.listen(this.abortController.signal)
  }

  stop(): void {
    this.running = false
    this.abortController?.abort()
    this.abortController = null
  }

  wake(): void {
    if (!this.running) return
    void this.reconcilePendingCompletions()
  }

  async attachAcceptance(input: {
    callbackSessionId: string
    connectionId: string
    chatId: number
    originalMessageId: number
    acceptedMessageId: number
  }): Promise<number> {
    const tasks = await listTasks(this.options.tasksDir)
    let patched = 0
    for (const task of tasks) {
      if (task.frontmatter.kind !== 'background') continue
      if (task.frontmatter.callback_session_id !== input.callbackSessionId) continue
      if (task.frontmatter.telegram_accepted_message_id) continue
      if (!['queued', 'running', 'callback_pending', 'done'].includes(task.frontmatter.status)) continue

      const updated = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        telegram_connection_id: input.connectionId,
        telegram_chat_id: String(input.chatId),
        telegram_original_message_id: String(input.originalMessageId),
        telegram_accepted_message_id: String(input.acceptedMessageId),
      })
      this.runtime.events.emit({
        type: EVENT_TYPES.TASK_METADATA_UPDATED,
        agent_id: updated.frontmatter.callback_agent_id ?? 'telegram-task-bridge',
        session_id: updated.frontmatter.callback_session_id ?? 'tasks',
        payload: {
          taskId: updated.frontmatter.id,
          title: updated.frontmatter.title,
          status: updated.frontmatter.status,
          callbackAgentId: updated.frontmatter.callback_agent_id,
          callbackSessionId: updated.frontmatter.callback_session_id,
          outputNote: updated.frontmatter.output_note,
          outputArtifact: updated.frontmatter.output_artifact,
        },
        timestamp: Date.now(),
      })
      patched++
    }

    if (patched > 0) this.wake()
    return patched
  }

  private async listen(signal: AbortSignal): Promise<void> {
    try {
      for await (const event of this.runtime.events.subscribe({
        types: [EVENT_TYPES.TASK_COMPLETED, EVENT_TYPES.TASK_METADATA_UPDATED],
      })) {
        if (signal.aborted || !this.running) break
        const taskId = typeof event.payload === 'object' && event.payload && 'taskId' in event.payload
          ? String(event.payload.taskId)
          : undefined
        await this.tryDeliverCompletion(taskId)
      }
    } catch (err) {
      if (!signal.aborted) {
        logger.error({ err }, 'Telegram task bridge listener failed')
      }
    }
  }

  private async reconcilePendingCompletions(): Promise<void> {
    await this.tryDeliverCompletion()
  }

  private async tryDeliverCompletion(taskId?: string): Promise<void> {
    if (!this.running || this.processing) return
    this.processing = true
    try {
      const task = (await listTasks(this.options.tasksDir)).find((candidate) => (
        (!taskId || candidate.frontmatter.id === taskId) &&
        candidate.frontmatter.kind === 'background' &&
        candidate.frontmatter.status === 'done' &&
        Boolean(candidate.frontmatter.telegram_connection_id) &&
        Boolean(candidate.frontmatter.telegram_chat_id) &&
        Boolean(candidate.frontmatter.telegram_accepted_message_id) &&
        !candidate.frontmatter.telegram_completion_message_id
      ))
      if (!task) return

      const fm = task.frontmatter
      const connection = await this.runtime.repositories.telegram.getConnection(fm.telegram_connection_id!)
      if (!connection) return

      const callbackAgentId = fm.callback_agent_id
      const lastAssistant = callbackAgentId
        ? [...await this.runtime.repositories.items.listByAgent(callbackAgentId)]
          .reverse()
          .find((item) => item.type === 'message' && item.role === 'assistant')
        : null
      const text = truncateTelegram(lastAssistant?.content ?? fm.completion_note ?? `Task completed: ${fm.id}`)
      const messageId = await this.sendTelegramMessage(connection.botToken, Number(fm.telegram_chat_id), text, Number(fm.telegram_accepted_message_id))
      if (messageId == null) return

      await updateTask(this.options.tasksDir, fm.id, {
        telegram_completion_message_id: String(messageId),
      })

      if (callbackAgentId) {
        await this.runtime.repositories.telegram.createMessageLink({
          id: randomUUID(),
          connectionId: connection.id,
          telegramChatId: fm.telegram_chat_id!,
          telegramMessageId: messageId,
          sessionId: fm.callback_session_id ?? connection.userId,
          itemId: lastAssistant?.id ?? null,
          senderType: 'bot',
          createdAt: Date.now(),
        })
      }
    } catch (err) {
      logger.error({ err, taskId }, 'Telegram task completion delivery failed')
    } finally {
      this.processing = false
    }
  }

  private async sendTelegramMessage(
    botToken: string,
    chatId: number,
    text: string,
    replyToMessageId: number,
  ): Promise<number | null> {
    const token = this.loadSecret(botToken)
    const response = await fetch(`https://api.telegram.org/bot${token}/sendMessage`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        chat_id: chatId,
        text,
        reply_to_message_id: replyToMessageId,
      }),
    })
    const json = await response.json() as { ok?: boolean; result?: { message_id?: number } }
    return json.ok && typeof json.result?.message_id === 'number' ? json.result.message_id : null
  }

  private loadSecret(value: string): string {
    if (!value.startsWith('enc:')) return value
    if (!this.encKey) throw new Error('Encrypted Telegram token cannot be decrypted without ENCRYPTION_KEY')
    return decrypt(value, this.encKey)
  }
}

function truncateTelegram(text: string): string {
  const maxLength = 3900
  return text.length > maxLength
    ? `${text.slice(0, maxLength - 40).trimEnd()}\n\n[truncated]`
    : text
}
