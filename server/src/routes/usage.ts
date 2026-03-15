import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

export function usageRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // GET / — Return basic usage stats
  app.get('/', async (c) => {
    try {
      const userId = 'dev' // TODO: extract from auth middleware

      const sessions = await runtime.repositories.sessions.listByUser(userId)
      const totalSessions = sessions.length

      let totalAgents = 0
      let totalItems = 0

      for (const session of sessions) {
        const agents = await runtime.repositories.agents.listBySession(session.id)
        totalAgents += agents.length

        for (const agent of agents) {
          const items = await runtime.repositories.items.listByAgent(agent.id)
          totalItems += items.length
        }
      }

      return c.json({
        totalSessions,
        totalAgents,
        totalItems,
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
