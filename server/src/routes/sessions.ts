import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

export function sessionRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // GET / — List user's sessions
  app.get('/', async (c) => {
    try {
      const userId = 'dev' // TODO: extract from auth middleware
      const sessions = await runtime.repositories.sessions.listByUser(userId)
      return c.json(sessions)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // GET /:id — Get session with its agents
  app.get('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const session = await runtime.repositories.sessions.getById(id)

      if (!session) {
        return c.json({ error: `Session not found: ${id}` }, 404)
      }

      const items = await runtime.repositories.items.listBySession(id)

      return c.json({ ...session, items })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // PATCH /:id — Update title, status
  app.patch('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const body = await c.req.json<{
        title?: string
        status?: 'active' | 'archived'
      }>()

      const existing = await runtime.repositories.sessions.getById(id)
      if (!existing) {
        return c.json({ error: `Session not found: ${id}` }, 404)
      }

      const updated = await runtime.repositories.sessions.update(id, {
        title: body.title,
        status: body.status,
      })

      return c.json(updated)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // DELETE /:id — Delete session
  app.delete('/:id', async (c) => {
    try {
      const { id } = c.req.param()

      const existing = await runtime.repositories.sessions.getById(id)
      if (!existing) {
        return c.json({ error: `Session not found: ${id}` }, 404)
      }

      await runtime.repositories.sessions.delete(id)
      return c.json({ ok: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
