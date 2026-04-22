import { Hono } from 'hono'
import { logger } from '../lib/logger.js'
import { splitModelId } from '../lib/model.js'
import type { RuntimeContext } from '../lib/runtime.js'

type SessionEnv = { Variables: { userId: string } }

export function sessionRoutes(runtime: RuntimeContext): Hono<SessionEnv> {
  const app = new Hono<SessionEnv>()

  // GET / — List user's sessions
  app.get('/', async (c) => {
    try {
      const userId = c.get('userId') as string
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
      const agents = await runtime.repositories.agents.listBySession(id)
      const workflowRuns = runtime.workflows
        ? await runtime.workflows.repository.listBySession(id)
        : []

      return c.json({ ...session, items, agents, workflowRuns })
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

  // POST /:id/generate-title — Generate a title for the session using LLM
  app.post('/:id/generate-title', async (c) => {
    try {
      const { id } = c.req.param()

      const session = await runtime.repositories.sessions.getById(id)
      if (!session) {
        return c.json({ error: `Session not found: ${id}` }, 404)
      }

      // Get the first few messages from the session
      const items = await runtime.repositories.items.listBySession(id)
      const messages = items
        .filter((i) => i.type === 'message' && (i.role === 'user' || i.role === 'assistant'))
        .slice(0, 6)

      if (messages.length === 0) {
        return c.json({ error: 'No messages in session to generate title from' }, 400)
      }

      const conversationText = messages
        .map((m) => `${m.role}: ${typeof m.content === 'string' ? m.content : JSON.stringify(m.content)}`)
        .join('\n')
        .slice(0, 2000) // limit context size

      const model = runtime.config.defaultModel
      const provider = runtime.providers.resolve(model)

      const { model: modelName } = splitModelId(model)

      const response = await provider.generate({
        model: modelName,
        messages: [
          {
            role: 'user',
            content: `Generate a short title (max 6 words) for this conversation. Return ONLY the title, no quotes or extra text.\n\n${conversationText}`,
          },
        ],
        temperature: 0.3,
        max_tokens: 30,
      })

      const title = (typeof response.content === 'string' ? response.content : String(response.content)).trim()

      // Update the session title
      await runtime.repositories.sessions.update(id, { title })

      return c.json({ title })
    } catch (err) {
      logger.error(err, 'POST /:id/generate-title failed')
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
