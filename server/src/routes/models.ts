import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

export function modelRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // GET / — List models
  app.get('/', async (c) => {
    try {
      const models = await runtime.repositories.models.list()
      return c.json(models)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // POST / — Add model
  app.post('/', async (c) => {
    try {
      const body = await c.req.json<{
        provider: string
        modelName: string
        displayName?: string
        maxTokens?: number
        contextWindow?: number
      }>()

      if (!body.provider || !body.modelName) {
        return c.json({ error: 'provider and modelName are required' }, 400)
      }

      const model = await runtime.repositories.models.create({
        provider: body.provider,
        name: body.modelName,
        displayName: body.displayName,
        maxTokens: body.maxTokens,
        contextWindow: body.contextWindow,
      })

      return c.json(model, 201)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // PATCH /:id — Update model
  app.patch('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const body = await c.req.json<{
        displayName?: string
        maxTokens?: number
        contextWindow?: number
      }>()

      const existing = await runtime.repositories.models.getById(id)
      if (!existing) {
        return c.json({ error: `Model not found: ${id}` }, 404)
      }

      const updated = await runtime.repositories.models.update(id, {
        displayName: body.displayName,
        maxTokens: body.maxTokens,
        contextWindow: body.contextWindow,
      })

      return c.json(updated)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // GET /ollama — Discover available Ollama models
  app.get('/ollama', async (c) => {
    try {
      const baseUrl = c.req.query('base_url') || 'http://localhost:11434'
      const url = `${baseUrl.replace(/\/+$/, '')}/api/tags`

      const res = await fetch(url, { signal: AbortSignal.timeout(5000) })
      if (!res.ok) {
        return c.json([])
      }

      const data = (await res.json()) as { models?: Array<{ name: string; size: number; digest: string; modified_at: string }> }
      return c.json(data.models ?? [])
    } catch {
      // Ollama unreachable — return empty array
      return c.json([])
    }
  })

  // DELETE /:id — Delete model
  app.delete('/:id', async (c) => {
    try {
      const { id } = c.req.param()

      const existing = await runtime.repositories.models.getById(id)
      if (!existing) {
        return c.json({ error: `Model not found: ${id}` }, 404)
      }

      await runtime.repositories.models.delete(id)
      return c.json({ ok: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
