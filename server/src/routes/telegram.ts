import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'
import {
  TelegramService,
  type TelegramConnectInput,
  type UpdateTelegramConnectionInput,
  type TelegramUpdate,
} from '../services/telegram.js'

type TelegramEnv = { Variables: { userId: string } }

export function telegramRoutes(runtime: RuntimeContext): Hono<TelegramEnv> {
  const app = new Hono<TelegramEnv>()
  const telegram = new TelegramService(runtime)

  app.get('/', async (c) => {
    try {
      const userId = c.get('userId') as string
      return c.json(await telegram.listConnections(userId))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.post('/connect', async (c) => {
    try {
      const userId = c.get('userId') as string
      const body = await c.req.json<TelegramConnectInput>()
      const result = await telegram.connectBot(userId, body)
      return c.json(result, result.test.ok && result.webhook?.ok ? 201 : 400)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 400)
    }
  })

  app.patch('/:id', async (c) => {
    try {
      const userId = c.get('userId') as string
      const { id } = c.req.param()
      const body = await c.req.json<UpdateTelegramConnectionInput>()
      const updated = await telegram.updateConnection(id, userId, body)
      if (!updated) return c.json({ error: `Telegram connection not found: ${id}` }, 404)
      return c.json(updated)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 400)
    }
  })

  app.delete('/:id', async (c) => {
    try {
      const userId = c.get('userId') as string
      const { id } = c.req.param()
      const deleted = await telegram.deleteConnection(id, userId)
      if (!deleted) return c.json({ error: `Telegram connection not found: ${id}` }, 404)
      return c.json({ ok: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.post('/:id/test', async (c) => {
    try {
      const userId = c.get('userId') as string
      const { id } = c.req.param()
      const result = await telegram.testConnection(id, userId)
      return c.json(result, result.ok ? 200 : 400)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.get('/:id/webhook', async (c) => {
    try {
      const userId = c.get('userId') as string
      const { id } = c.req.param()
      const info = await telegram.getWebhookInfo(id, userId)
      if (!info) return c.json({ error: `Telegram connection not found: ${id}` }, 404)
      return c.json(info)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.post('/:id/webhook/register', async (c) => {
    try {
      const userId = c.get('userId') as string
      const { id } = c.req.param()
      const result = await telegram.registerWebhook(id, userId)
      if (!result) return c.json({ error: `Telegram connection not found: ${id}` }, 404)
      return c.json(result, result.ok ? 200 : 400)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 400)
    }
  })

  app.delete('/:id/webhook', async (c) => {
    try {
      const userId = c.get('userId') as string
      const { id } = c.req.param()
      const result = await telegram.deleteWebhook(id, userId)
      if (!result) return c.json({ error: `Telegram connection not found: ${id}` }, 404)
      return c.json(result, result.ok ? 200 : 400)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}

export function telegramWebhookRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()
  const telegram = new TelegramService(runtime)

  app.post('/webhook/:connectionId/:pathSecret', async (c) => {
    try {
      const { connectionId, pathSecret } = c.req.param()
      const headerSecret = c.req.header('x-telegram-bot-api-secret-token')
      const body = await c.req.json<TelegramUpdate>()
      const result = await telegram.processWebhook(connectionId, pathSecret, headerSecret, body)

      if (result.status === 'rejected') {
        const status = result.reason === 'connection_not_found' ? 404 : 403
        return c.json(result, status)
      }
      return c.json(result, 200)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
