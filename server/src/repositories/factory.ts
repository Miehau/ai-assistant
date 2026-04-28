import { logger } from '../lib/logger.js'
import type { AppConfig } from '../lib/config.js'
import { createDatabase, SQLiteRepositories, type DrizzleInstance as SqliteDb } from './sqlite/index.js'
import { createPgDatabase, PostgresRepositories, type PgDrizzleInstance } from './postgres/index.js'
import type {
  AgentRepository,
  ItemRepository,
  SessionRepository,
  UserRepository,
  ToolOutputRepository,
  ModelRepository,
  ApiKeyRepository,
  SystemPromptRepository,
  PreferenceRepository,
  WorkflowRunRepository,
  McpRepository,
  TelegramRepository,
} from './types.js'

export interface RepositoryBundle {
  users: UserRepository
  sessions: SessionRepository
  agents: AgentRepository
  items: ItemRepository
  toolOutputs: ToolOutputRepository
  models: ModelRepository
  apiKeys: ApiKeyRepository
  systemPrompts: SystemPromptRepository
  preferences: PreferenceRepository
  workflowRuns: WorkflowRunRepository
  mcp: McpRepository
  telegram: TelegramRepository
}

export interface OpenedDb {
  dialect: 'sqlite' | 'postgres'
  /** Drizzle handle — type depends on dialect. SQLite is sync, Postgres is async. */
  db: SqliteDb | PgDrizzleInstance
  repositories: RepositoryBundle
  /** Best-effort close — sqlite is a no-op, postgres ends the pool. */
  close(): Promise<void>
}

export async function openDatabase(config: AppConfig): Promise<OpenedDb> {
  if (config.dbDialect === 'postgres') {
    logger.info({ url: redactDatabaseUrl(config.databaseUrl) }, 'Opening postgres database')
    const opened = await createPgDatabase(config.databaseUrl)
    const repos = new PostgresRepositories(opened.db, config.encryptionKey)
    return {
      dialect: 'postgres',
      db: opened.db,
      repositories: repos,
      close: opened.close,
    }
  }

  logger.info({ path: config.databaseUrl }, 'Opening sqlite database')
  const db = createDatabase(config.databaseUrl)
  const repos = new SQLiteRepositories(db, config.encryptionKey)
  return {
    dialect: 'sqlite',
    db,
    repositories: repos,
    close: async () => {
      // better-sqlite3 has no async close path; rely on process exit
    },
  }
}

function redactDatabaseUrl(value: string): string {
  try {
    const url = new URL(value)
    if (url.password) url.password = '***'
    if (url.username) url.username = url.username ? '***' : ''
    return url.toString()
  } catch {
    return value
  }
}
