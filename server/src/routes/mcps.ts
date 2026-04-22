import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

export function mcpRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  app.get('/', async (c) => {
    try {
      return c.json(await runtime.mcps.listServers())
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.post('/', async (c) => {
    try {
      const body = await c.req.json()
      return c.json(await runtime.mcps.createServer(body), 201)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 400)
    }
  })

  app.patch('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const body = await c.req.json()
      return c.json(await runtime.mcps.updateServer(id, body))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 400)
    }
  })

  app.delete('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const deleted = await runtime.mcps.deleteServer(id)
      if (!deleted) return c.json({ error: `MCP server not found: ${id}` }, 404)
      return c.json({ ok: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.patch('/:id/tools/:toolName', async (c) => {
    try {
      const { id, toolName } = c.req.param()
      const body = await c.req.json<{ enabledForNewSessions?: boolean }>()
      if (typeof body.enabledForNewSessions !== 'boolean') {
        return c.json({ error: 'enabledForNewSessions is required' }, 400)
      }
      return c.json(await runtime.mcps.setToolEnabled(
        id,
        decodeURIComponent(toolName),
        body.enabledForNewSessions,
      ))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 400)
    }
  })

  return app
}
