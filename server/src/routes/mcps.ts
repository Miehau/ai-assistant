import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

type McpRouteEnv = { Variables: { userId: string } }

export function mcpRoutes(runtime: RuntimeContext): Hono<McpRouteEnv> {
  const app = new Hono<McpRouteEnv>()

  app.get('/', async (c) => {
    try {
      return c.json(await runtime.mcps.listServers(c.get('userId')))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.post('/', async (c) => {
    try {
      const body = await c.req.json()
      return c.json(await runtime.mcps.createServer(c.get('userId'), body), 201)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 400)
    }
  })

  app.patch('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const body = await c.req.json()
      return c.json(await runtime.mcps.updateServer(c.get('userId'), id, body))
    } catch (err) {
      return lifecycleError(c, err)
    }
  })

  app.delete('/:id', async (c) => {
    try {
      const { id } = c.req.param()
      const deleted = await runtime.mcps.deleteServer(c.get('userId'), id)
      if (!deleted) return c.json({ error: `MCP server not found: ${id}` }, 404)
      return c.json({ ok: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  app.post('/:id/connect', async (c) => {
    try {
      return c.json(await runtime.mcps.connectServer(c.get('userId'), c.req.param('id')))
    } catch (error) {
      return lifecycleError(c, error)
    }
  })

  app.post('/:id/reconnect', async (c) => {
    try {
      return c.json(await runtime.mcps.connectServer(c.get('userId'), c.req.param('id')))
    } catch (error) {
      return lifecycleError(c, error)
    }
  })

  app.post('/:id/disconnect', async (c) => {
    try {
      const server = await runtime.mcps.getServer(c.get('userId'), c.req.param('id'))
      if (!server) return c.json({ error: 'MCP server not found' }, 404)
      return c.json(await runtime.mcps.disconnectServer(
        c.get('userId'), server.id, ['auto', 'oauth'].includes(server.authMode),
      ))
    } catch (error) {
      return lifecycleError(c, error)
    }
  })

  app.post('/:id/oauth/start', async (c) => {
    try {
      const serverId = c.req.param('id')
      const pending = await runtime.mcps.startOAuth(c.get('userId'), serverId)
      if (!pending) return c.json({ error: 'Authorization is not required or could not be started' }, 409)
      return c.json({
        session: {
          id: pending.sessionId,
          serverId,
          status: 'pending' as const,
          authorizationUrl: pending.authorizationUrl,
          expiresAt: pending.expiresAt,
          error: null,
        },
      })
    } catch (error) {
      return lifecycleError(c, error)
    }
  })

  app.get('/:id/oauth/session', async (c) => {
    const server = await runtime.mcps.getServer(c.get('userId'), c.req.param('id'))
    if (!server) return c.json({ error: 'MCP server not found' }, 404)
    return c.json({ session: server.oauthSession })
  })

  app.post('/:id/oauth/cancel', async (c) => {
    try {
      return c.json(await runtime.mcps.cancelOAuth(c.get('userId'), c.req.param('id')))
    } catch (error) {
      return lifecycleError(c, error)
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
        c.get('userId'),
        id,
        decodeURIComponent(toolName),
        body.enabledForNewSessions,
      ))
    } catch (err) {
      return lifecycleError(c, err)
    }
  })

  return app
}

function lifecycleError(c: any, error: unknown) {
  const message = error instanceof Error ? error.message : 'MCP lifecycle operation failed'
  if (message.toLowerCase().includes('not found')) return c.json({ error: 'MCP server not found' }, 404)
  if (message.includes('PUBLIC_BASE_URL') || message.includes('ENCRYPTION_KEY') || message.includes('not available')) {
    return c.json({ error: message }, 400)
  }
  return c.json({ error: message }, 502)
}
