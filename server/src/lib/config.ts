import { z } from 'zod'

const boolFromEnv = z.preprocess((v) => {
  if (typeof v !== 'string') return v
  return v.toLowerCase() === 'true' || v === '1'
}, z.boolean())

const configSchema = z.object({
  nodeEnv: z.enum(['development', 'production', 'test']).default('development'),
  port: z.coerce.number().default(3001),
  host: z.string().default('localhost'),
  dbDialect: z.enum(['sqlite', 'postgres']).default('sqlite'),
  databaseUrl: z.string().default('./data/app.db'),
  defaultModel: z.string().default('anthropic:claude-sonnet-4-20250514'),
  publicBaseUrl: z.string().optional(),
  encryptionKey: z.string().optional(),
  anthropicApiKey: z.string().optional(),
  openaiApiKey: z.string().optional(),
  ollamaBaseUrl: z.string().optional(),
  openrouterApiKey: z.string().optional(),
  workingDir: z.string().optional(),
  agentsDir: z.string().default('./agents'),
  tasksDir: z.string().default('./data/tasks'),
  workspaceDir: z.string().default('./data/workspace'),
  sessionFilesDir: z.string().default('./data/sessions'),
  inlineOutputLimitBytes: z.coerce.number().default(32 * 1024),
  workflowsDir: z.string().default('./workflows'),
  // --- security / hardening ---
  allowedOrigins: z.string().default(''),
  trustProxy: boolFromEnv.default(false),
  enableShellTool: boolFromEnv.default(false),
  rateLimitAuthFailurePerMin: z.coerce.number().default(120),
  rateLimitApiPerMin: z.coerce.number().default(60),
  rateLimitInferencePerMin: z.coerce.number().default(60),
  rateLimitTelegramPerMin: z.coerce.number().default(600),
  rateLimitHealthPerMin: z.coerce.number().default(20),
})

export type AppConfig = z.infer<typeof configSchema>

export function loadConfig(): AppConfig {
  const config = configSchema.parse({
    nodeEnv: process.env.NODE_ENV,
    port: process.env.PORT,
    host: process.env.HOST,
    dbDialect: process.env.DB_DIALECT,
    databaseUrl: process.env.DATABASE_URL,
    defaultModel: process.env.DEFAULT_MODEL,
    publicBaseUrl: process.env.PUBLIC_BASE_URL,
    encryptionKey: process.env.ENCRYPTION_KEY,
    anthropicApiKey: process.env.ANTHROPIC_API_KEY,
    openaiApiKey: process.env.OPENAI_API_KEY,
    ollamaBaseUrl: process.env.OLLAMA_BASE_URL,
    openrouterApiKey: process.env.OPENROUTER_API_KEY,
    workingDir: process.env.WORKING_DIR,
    agentsDir: process.env.AGENTS_DIR,
    tasksDir: process.env.TASKS_DIR,
    workspaceDir: process.env.WORKSPACE_DIR,
    sessionFilesDir: process.env.SESSION_FILES_DIR,
    inlineOutputLimitBytes: process.env.INLINE_OUTPUT_LIMIT_BYTES,
    workflowsDir: process.env.WORKFLOWS_DIR,
    allowedOrigins: process.env.ALLOWED_ORIGINS,
    trustProxy: process.env.TRUST_PROXY,
    enableShellTool: process.env.ENABLE_SHELL_TOOL,
    rateLimitAuthFailurePerMin: process.env.RATE_LIMIT_AUTH_FAILURE_PER_MIN,
    rateLimitApiPerMin: process.env.RATE_LIMIT_API_PER_MIN,
    rateLimitInferencePerMin: process.env.RATE_LIMIT_INFERENCE_PER_MIN,
    rateLimitTelegramPerMin: process.env.RATE_LIMIT_TELEGRAM_PER_MIN,
    rateLimitHealthPerMin: process.env.RATE_LIMIT_HEALTH_PER_MIN,
  })

  if (
    config.dbDialect === 'postgres' &&
    !/^postgres(?:ql)?:\/\//i.test(config.databaseUrl)
  ) {
    throw new Error('DB_DIALECT=postgres requires DATABASE_URL to be a postgres:// or postgresql:// connection string')
  }

  return config
}
