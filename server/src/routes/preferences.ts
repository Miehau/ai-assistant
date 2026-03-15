import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

export function preferenceRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // GET /:key — Get preference value
  app.get('/:key', async (c) => {
    try {
      const { key } = c.req.param()
      const value = await runtime.repositories.preferences.get(key)

      if (value === null) {
        return c.json({ key, value: null }, 404)
      }

      return c.json({ key, value })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // PUT /:key — Set preference value
  app.put('/:key', async (c) => {
    try {
      const { key } = c.req.param()
      const body = await c.req.json<{ value: string }>()

      if (body.value === undefined || body.value === null) {
        return c.json({ error: 'value is required' }, 400)
      }

      await runtime.repositories.preferences.set(key, body.value)
      return c.json({ key, value: body.value })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
