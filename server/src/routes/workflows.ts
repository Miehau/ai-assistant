import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'

type WorkflowEnv = { Variables: { userId: string } }

export function workflowRoutes(runtime: RuntimeContext) {
  const app = new Hono<WorkflowEnv>()

  // GET / — List registered workflow definitions
  app.get('/', (c) => {
    if (!runtime.workflows) {
      return c.json([])
    }
    const defs = runtime.workflows.registry.list().map((d) => ({
      name: d.name,
      description: d.description,
      tools: d.tools ?? null,
    }))
    return c.json(defs)
  })

  // GET /runs/:runId — Get run status + output
  app.get('/runs/:runId', async (c) => {
    if (!runtime.workflows) {
      return c.json({ error: 'Workflow system not initialized' }, 500)
    }
    const { runId } = c.req.param()
    const run = await runtime.workflows.repository.getById(runId)
    if (!run) {
      return c.json({ error: `Run not found: ${runId}` }, 404)
    }
    return c.json(run)
  })

  // DELETE /runs/:runId/cancel — Cancel a running workflow
  app.delete('/runs/:runId/cancel', async (c) => {
    if (!runtime.workflows) {
      return c.json({ error: 'Workflow system not initialized' }, 500)
    }
    const { runId } = c.req.param()
    await runtime.workflows.executor.cancel(runId)
    return c.json({ ok: true })
  })

  return app
}
