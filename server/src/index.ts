import 'dotenv/config'
import { serve } from '@hono/node-server'
import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { logger as honoLogger } from 'hono/logger'
import { logger } from './lib/logger.js'
import { loadConfig } from './lib/config.js'
import { initRuntime, shutdownRuntime } from './lib/runtime.js'
import type { RuntimeContext } from './lib/runtime.js'
import { chatRoutes } from './routes/chat.js'
import { sessionRoutes } from './routes/sessions.js'
import { modelRoutes } from './routes/models.js'
import { apiKeyRoutes } from './routes/api-keys.js'
import { systemPromptRoutes } from './routes/system-prompts.js'
import { preferenceRoutes } from './routes/preferences.js'
import { toolRoutes } from './routes/tools.js'
import { usageRoutes } from './routes/usage.js'
import { openaiCompatRoutes } from './routes/openai-compat.js'
import { authMiddleware } from './middleware/auth.js'

type AppEnv = {
  Variables: {
    runtime: RuntimeContext
    userId: string
  }
}

const config = loadConfig()

async function main() {
  const runtime = await initRuntime(config)

  const app = new Hono<AppEnv>()

  // Middleware
  app.use('*', cors())
  app.use('*', honoLogger())

  // Inject runtime into context
  app.use('*', async (c, next) => {
    c.set('runtime', runtime)
    await next()
  })

  // Health check (no auth)
  app.get('/health', (c) => c.json({ status: 'ok', timestamp: Date.now() }))

  // Auth for API routes (not /health, not /v1/*)
  app.use('/api/*', authMiddleware)

  // API Routes
  app.route('/api/chat', chatRoutes(runtime))
  app.route('/api/sessions', sessionRoutes(runtime))
  app.route('/api/models', modelRoutes(runtime))
  app.route('/api/keys', apiKeyRoutes(runtime))
  app.route('/api/system-prompts', systemPromptRoutes(runtime))
  app.route('/api/preferences', preferenceRoutes(runtime))
  app.route('/api/tools', toolRoutes(runtime))
  app.route('/api/usage', usageRoutes(runtime))

  // OpenAI-compatible endpoint — lets the frontend use this server as a "custom backend"
  // Register in the UI as Custom Backend with URL: http://localhost:3001/v1/chat/completions
  app.route('/v1', openaiCompatRoutes(runtime))

  // Start server
  serve({
    fetch: app.fetch,
    port: config.port,
    hostname: config.host,
  })

  logger.info({ port: config.port, host: config.host }, 'Server started')

  // Graceful shutdown
  process.on('SIGINT', async () => {
    logger.info('Shutting down...')
    await shutdownRuntime(runtime)
    process.exit(0)
  })
}

main().catch((err) => {
  logger.error(err, 'Failed to start server')
  process.exit(1)
})
