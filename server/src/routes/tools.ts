import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

export function toolRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // GET / — List all tools with metadata
  app.get('/', (c) => {
    try {
      const tools = runtime.tools.listMetadata()
      return c.json(tools)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
