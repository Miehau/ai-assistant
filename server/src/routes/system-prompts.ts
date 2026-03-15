import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

export function systemPromptRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // GET / — List all system prompts
  app.get('/', async (c) => {
    try {
      const prompts = await runtime.repositories.systemPrompts.list()
      return c.json(prompts)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // GET /:id — Get by id
  app.get('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const prompt = await runtime.repositories.systemPrompts.getById(id)

      if (!prompt) {
        return c.json({ error: `System prompt not found: ${id}` }, 404)
      }

      return c.json(prompt)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // POST / — Create
  app.post('/', async (c) => {
    try {
      const body = await c.req.json<{
        name: string
        content: string
        isDefault?: boolean
      }>()

      if (!body.name || !body.content) {
        return c.json({ error: 'name and content are required' }, 400)
      }

      const prompt = await runtime.repositories.systemPrompts.create({
        name: body.name,
        content: body.content,
        isDefault: body.isDefault,
      })

      return c.json(prompt, 201)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // PATCH /:id — Update
  app.patch('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const body = await c.req.json<{
        name?: string
        content?: string
        isDefault?: boolean
      }>()

      const existing = await runtime.repositories.systemPrompts.getById(id)
      if (!existing) {
        return c.json({ error: `System prompt not found: ${id}` }, 404)
      }

      const updated = await runtime.repositories.systemPrompts.update(id, {
        name: body.name,
        content: body.content,
        isDefault: body.isDefault,
      })

      return c.json(updated)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // DELETE /:id — Delete
  app.delete('/:id', async (c) => {
    try {
      const { id } = c.req.param()

      const existing = await runtime.repositories.systemPrompts.getById(id)
      if (!existing) {
        return c.json({ error: `System prompt not found: ${id}` }, 404)
      }

      await runtime.repositories.systemPrompts.delete(id)
      return c.json({ ok: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
