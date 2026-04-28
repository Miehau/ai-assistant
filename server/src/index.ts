import 'dotenv/config'
import path from 'path'
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
import { workflowRoutes } from './routes/workflows.js'
import { openaiCompatRoutes } from './routes/openai-compat.js'
import { authMiddleware } from './middleware/auth.js'
import { mcpRoutes } from './routes/mcps.js'
import { telegramRoutes, telegramWebhookRoutes } from './routes/telegram.js'
import { createRateLimiter } from './lib/rate-limit.js'

type AppEnv = {
  Variables: {
    runtime: RuntimeContext
    userId: string
  }
}

const config = loadConfig()

if (config.workingDir) {
  // Resolve DATABASE_URL to absolute before chdir so it doesn't get re-rooted
  if (config.dbDialect === 'sqlite' && !path.isAbsolute(config.databaseUrl)) {
    config.databaseUrl = path.resolve(process.cwd(), config.databaseUrl)
  }
  process.chdir(config.workingDir)
}

async function main() {
  const runtime = await initRuntime(config)

  const app = new Hono<AppEnv>()

  // CORS — bearer-token auth is the gate; CORS stays open without credentials
  // so the bearer-Authorization model works from any origin (Tauri has no fixed origin).
  // The actual gate is authMiddleware below.
  const allowedOrigins = config.allowedOrigins
    .split(',')
    .map((o) => o.trim())
    .filter(Boolean)
  app.use(
    '*',
    cors(
      allowedOrigins.length > 0
        ? { origin: allowedOrigins, credentials: true }
        : { origin: '*', credentials: false },
    ),
  )
  app.use('*', honoLogger())

  // Inject runtime into context
  app.use('*', async (c, next) => {
    c.set('runtime', runtime)
    await next()
  })

  // Health check (no auth, IP-based rate limit)
  app.use(
    '/health',
    createRateLimiter({
      name: 'health',
      limit: config.rateLimitHealthPerMin,
      keyBy: 'ip',
      trustProxy: config.trustProxy,
    }),
  )
  app.get('/health', (c) => c.json({ status: 'ok', timestamp: Date.now() }))

  // Telegram webhooks — server-to-server from Telegram, IP-based rate limit
  app.use(
    '/telegram/*',
    createRateLimiter({
      name: 'telegram',
      limit: config.rateLimitTelegramPerMin,
      keyBy: 'ip',
      trustProxy: config.trustProxy,
    }),
  )
  app.route('/telegram', telegramWebhookRoutes(runtime))

  // Pre-auth failure throttle, then auth + per-user rate limit for /api/*
  app.use(
    '/api/*',
    createRateLimiter({
      name: 'api-auth-failure',
      limit: config.rateLimitAuthFailurePerMin,
      keyBy: 'ip',
      trustProxy: config.trustProxy,
      skipSuccessfulRequests: true,
    }),
  )
  app.use('/api/*', authMiddleware)
  app.use(
    '/api/*',
    createRateLimiter({
      name: 'api',
      limit: config.rateLimitApiPerMin,
      keyBy: 'user',
      trustProxy: config.trustProxy,
    }),
  )

  // API Routes
  app.route('/api/chat', chatRoutes(runtime))
  app.route('/api/sessions', sessionRoutes(runtime))
  app.route('/api/models', modelRoutes(runtime))
  app.route('/api/keys', apiKeyRoutes(runtime))
  app.route('/api/system-prompts', systemPromptRoutes(runtime))
  app.route('/api/preferences', preferenceRoutes(runtime))
  app.route('/api/tools', toolRoutes(runtime))
  app.route('/api/usage', usageRoutes(runtime))
  app.route('/api/workflows', workflowRoutes(runtime))
  app.route('/api/mcps', mcpRoutes(runtime))
  app.route('/api/telegram', telegramRoutes(runtime))

  // OpenAI-compatible endpoint — gated like /api/* with its own rate-limit bucket
  app.use(
    '/v1/*',
    createRateLimiter({
      name: 'inference-auth-failure',
      limit: config.rateLimitAuthFailurePerMin,
      keyBy: 'ip',
      trustProxy: config.trustProxy,
      skipSuccessfulRequests: true,
    }),
  )
  app.use('/v1/*', authMiddleware)
  app.use(
    '/v1/*',
    createRateLimiter({
      name: 'inference',
      limit: config.rateLimitInferencePerMin,
      keyBy: 'user',
      trustProxy: config.trustProxy,
    }),
  )
  app.route('/v1', openaiCompatRoutes(runtime))

  // Start server
  const server = serve({
    fetch: app.fetch,
    port: config.port,
    hostname: config.host,
  })

  logger.info({ port: config.port, host: config.host }, 'Server started')

  let shuttingDown = false
  const shutdown = async (signalName: string) => {
    if (shuttingDown) return
    shuttingDown = true
    logger.info({ signal: signalName }, 'Shutting down...')
    server.close()
    await shutdownRuntime(runtime)
    process.exit(0)
  }

  // Graceful shutdown
  process.on('SIGINT', () => { void shutdown('SIGINT') })
  process.on('SIGTERM', () => { void shutdown('SIGTERM') })
}

main().catch((err) => {
  logger.error(err, 'Failed to start server')
  process.exit(1)
})
