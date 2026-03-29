import { z } from 'zod'

const configSchema = z.object({
  port: z.coerce.number().default(3001),
  host: z.string().default('localhost'),
  databaseUrl: z.string().default('./data/app.db'),
  defaultModel: z.string().default('anthropic:claude-sonnet-4-20250514'),
  encryptionKey: z.string().optional(),
  anthropicApiKey: z.string().optional(),
  openaiApiKey: z.string().optional(),
  ollamaBaseUrl: z.string().optional(),
  openrouterApiKey: z.string().optional(),
  workingDir: z.string().optional(),
  agentsDir: z.string().default('./agents'),
  tasksDir: z.string().default('./data/tasks'),
  workspaceDir: z.string().default('./data/workspace'),
})

export type AppConfig = z.infer<typeof configSchema>

export function loadConfig(): AppConfig {
  return configSchema.parse({
    port: process.env.PORT,
    host: process.env.HOST,
    databaseUrl: process.env.DATABASE_URL,
    defaultModel: process.env.DEFAULT_MODEL,
    encryptionKey: process.env.ENCRYPTION_KEY,
    anthropicApiKey: process.env.ANTHROPIC_API_KEY,
    openaiApiKey: process.env.OPENAI_API_KEY,
    ollamaBaseUrl: process.env.OLLAMA_BASE_URL,
    openrouterApiKey: process.env.OPENROUTER_API_KEY,
    workingDir: process.env.WORKING_DIR,
    agentsDir: process.env.AGENTS_DIR,
    tasksDir: process.env.TASKS_DIR,
    workspaceDir: process.env.WORKSPACE_DIR,
  })
}
