import Database from 'better-sqlite3'
import { drizzle, type BetterSQLite3Database } from 'drizzle-orm/better-sqlite3'
import { eq, and, desc, asc, sql, max, isNull } from 'drizzle-orm'
import { v4 as uuid } from 'uuid'
import { createHash } from 'crypto'
import { encrypt, decrypt, deriveKey } from '../../lib/crypto.js'

import * as schema from '../../db/schema.js'
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
  CreateAgentInput,
  UpdateAgentInput,
  CreateSessionInput,
  UpdateSessionInput,
  CreateUserInput,
  CreateItemInput,
  SaveToolOutputInput,
  CreateModelInput,
  UpdateModelInput,
  ModelRecord,
  ApiKeyRecord,
  SystemPromptRecord,
  CreateSystemPromptInput,
  UpdateSystemPromptInput,
} from '../types.js'
import type {
  Agent,
  AgentConfig,
  AgentStatus,
  Item,
  ItemRole,
  ItemType,
  Plan,
  Session,
  SessionStatus,
  ToolOutput,
  User,
  WaitingFor,
} from '../../domain/types.js'

export type DrizzleInstance = BetterSQLite3Database<typeof schema>

// --- Helpers ---

function toAgent(row: typeof schema.agents.$inferSelect): Agent {
  return {
    id: row.id,
    sessionId: row.sessionId!,
    parentId: row.parentId ?? null,
    sourceCallId: row.sourceCallId ?? null,
    depth: row.depth ?? 0,
    task: row.task,
    config: JSON.parse(row.config ?? '{}') as AgentConfig,
    status: (row.status ?? 'pending') as AgentStatus,
    waitingFor: JSON.parse(row.waitingFor ?? '[]') as WaitingFor[],
    result: row.result ?? null,
    error: row.error ?? null,
    turnCount: row.turnCount ?? 0,
    plan: row.plan ? (JSON.parse(row.plan) as Plan) : null,
    createdAt: row.createdAt ?? 0,
    updatedAt: row.updatedAt ?? 0,
    completedAt: row.completedAt ?? null,
  }
}

function toSession(row: typeof schema.sessions.$inferSelect): Session {
  return {
    id: row.id,
    userId: row.userId!,
    rootAgentId: row.rootAgentId ?? null,
    title: row.title ?? null,
    summary: row.summary ?? null,
    status: (row.status ?? 'active') as SessionStatus,
    createdAt: row.createdAt ?? 0,
    updatedAt: row.updatedAt ?? 0,
  }
}

function toUser(row: typeof schema.users.$inferSelect): User {
  return {
    id: row.id,
    email: row.email ?? null,
    apiKeyHash: row.apiKeyHash,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  }
}

function toItem(row: typeof schema.items.$inferSelect): Item {
  return {
    id: row.id,
    agentId: row.agentId!,
    sequence: row.sequence,
    type: row.type as ItemType,
    role: (row.role as ItemRole) ?? null,
    content: row.content ?? null,
    callId: row.callId ?? null,
    name: row.name ?? null,
    arguments: row.arguments ?? null,
    output: row.output ?? null,
    isError: row.isError != null ? Boolean(row.isError) : null,
    saveOutput: row.saveOutput != null ? Boolean(row.saveOutput) : null,
    turnNumber: row.turnNumber ?? 0,
    durationMs: row.durationMs ?? null,
    createdAt: row.createdAt,
  }
}

function toToolOutput(row: typeof schema.toolOutputs.$inferSelect): ToolOutput {
  return {
    id: row.id,
    agentId: row.agentId!,
    callId: row.callId!,
    toolName: row.toolName,
    data: row.data ? JSON.parse(row.data) : null,
    createdAt: row.createdAt,
  }
}

function toModelRecord(row: typeof schema.models.$inferSelect): ModelRecord {
  return {
    id: row.id,
    provider: row.provider,
    name: row.modelName,
    displayName: null,
    maxTokens: null,
    contextWindow: null,
    createdAt: row.createdAt,
    updatedAt: row.createdAt,
  }
}

function toApiKeyRecord(row: typeof schema.apiKeys.$inferSelect): ApiKeyRecord {
  return {
    provider: row.provider,
    encryptedKey: row.apiKey,
    createdAt: row.createdAt,
    updatedAt: row.createdAt,
  }
}

function toSystemPromptRecord(row: typeof schema.systemPrompts.$inferSelect): SystemPromptRecord {
  return {
    id: row.id,
    name: row.name,
    content: row.content,
    isDefault: false,
    createdAt: row.createdAt ?? 0,
    updatedAt: row.updatedAt ?? 0,
  }
}

// --- Repository factories ---

function createUserRepo(db: DrizzleInstance): UserRepository {
  return {
    async create(input: CreateUserInput): Promise<User> {
      const now = Date.now()
      const id = uuid()
      const row = {
        id,
        email: input.email ?? null,
        apiKeyHash: input.apiKeyHash,
        createdAt: now,
        updatedAt: now,
      }
      db.insert(schema.users).values(row).run()
      return toUser(row)
    },

    async getById(id: string): Promise<User | null> {
      const rows = db.select().from(schema.users).where(eq(schema.users.id, id)).limit(1).all()
      return rows.length > 0 ? toUser(rows[0]) : null
    },

    async getByApiKeyHash(apiKeyHash: string): Promise<User | null> {
      const rows = db
        .select()
        .from(schema.users)
        .where(eq(schema.users.apiKeyHash, apiKeyHash))
        .limit(1)
        .all()
      return rows.length > 0 ? toUser(rows[0]) : null
    },
  }
}

function createSessionRepo(db: DrizzleInstance): SessionRepository {
  return {
    async create(input: CreateSessionInput): Promise<Session> {
      const now = Date.now()
      const id = uuid()
      const row = {
        id,
        userId: input.userId,
        rootAgentId: null,
        title: input.title ?? null,
        summary: null,
        status: 'active' as const,
        createdAt: now,
        updatedAt: now,
      }
      db.insert(schema.sessions).values(row).run()
      return toSession(row)
    },

    async getById(id: string): Promise<Session | null> {
      const rows = db
        .select()
        .from(schema.sessions)
        .where(eq(schema.sessions.id, id))
        .limit(1)
        .all()
      return rows.length > 0 ? toSession(rows[0]) : null
    },

    async listByUser(userId: string): Promise<Session[]> {
      const rows = db
        .select()
        .from(schema.sessions)
        .where(eq(schema.sessions.userId, userId))
        .orderBy(desc(schema.sessions.createdAt))
        .all()
      return rows.map(toSession)
    },

    async update(id: string, input: UpdateSessionInput): Promise<Session> {
      const now = Date.now()
      const updates: Record<string, unknown> = { updatedAt: now }
      if (input.rootAgentId !== undefined) updates.rootAgentId = input.rootAgentId
      if (input.title !== undefined) updates.title = input.title
      if (input.summary !== undefined) updates.summary = input.summary
      if (input.status !== undefined) updates.status = input.status

      db.update(schema.sessions).set(updates).where(eq(schema.sessions.id, id)).run()

      const rows = db
        .select()
        .from(schema.sessions)
        .where(eq(schema.sessions.id, id))
        .limit(1)
        .all()
      if (rows.length === 0) throw new Error(`Session not found: ${id}`)
      return toSession(rows[0])
    },

    async delete(id: string): Promise<void> {
      db.transaction((tx) => {
        const agentRows = tx
          .select({ id: schema.agents.id })
          .from(schema.agents)
          .where(eq(schema.agents.sessionId, id))
          .all()

        for (const agent of agentRows) {
          tx.delete(schema.items).where(eq(schema.items.agentId, agent.id)).run()
          tx.delete(schema.toolOutputs).where(eq(schema.toolOutputs.agentId, agent.id)).run()
        }

        tx.delete(schema.agents).where(eq(schema.agents.sessionId, id)).run()
        tx.delete(schema.sessions).where(eq(schema.sessions.id, id)).run()
      })
    },
  }
}

function createAgentRepo(db: DrizzleInstance): AgentRepository {
  return {
    async create(input: CreateAgentInput): Promise<Agent> {
      const now = Date.now()
      const id = uuid()
      const row = {
        id,
        sessionId: input.sessionId,
        parentId: input.parentId ?? null,
        sourceCallId: input.sourceCallId ?? null,
        depth: input.depth ?? 0,
        task: input.task,
        config: JSON.stringify(input.config),
        status: 'pending' as const,
        waitingFor: '[]',
        result: null,
        error: null,
        turnCount: 0,
        plan: null,
        createdAt: now,
        updatedAt: now,
        completedAt: null,
      }
      db.insert(schema.agents).values(row).run()
      return toAgent(row)
    },

    async getById(id: string): Promise<Agent | null> {
      const rows = db.select().from(schema.agents).where(eq(schema.agents.id, id)).limit(1).all()
      return rows.length > 0 ? toAgent(rows[0]) : null
    },

    async update(id: string, input: UpdateAgentInput): Promise<Agent> {
      const now = Date.now()
      const updates: Record<string, unknown> = { updatedAt: now }
      if (input.status !== undefined) updates.status = input.status
      if (input.waitingFor !== undefined) updates.waitingFor = JSON.stringify(input.waitingFor)
      if (input.result !== undefined) updates.result = input.result
      if (input.error !== undefined) updates.error = input.error
      if (input.turnCount !== undefined) updates.turnCount = input.turnCount
      if (input.plan !== undefined) updates.plan = input.plan ? JSON.stringify(input.plan) : null
      if (input.completedAt !== undefined) updates.completedAt = input.completedAt

      db.update(schema.agents).set(updates).where(eq(schema.agents.id, id)).run()

      const rows = db.select().from(schema.agents).where(eq(schema.agents.id, id)).limit(1).all()
      if (rows.length === 0) throw new Error(`Agent not found: ${id}`)
      return toAgent(rows[0])
    },

    async findWaitingForCall(callId: string): Promise<Agent | null> {
      const rows = db
        .select()
        .from(schema.agents)
        .where(
          sql`EXISTS (SELECT 1 FROM json_each(${schema.agents.waitingFor}) WHERE json_extract(value, '$.callId') = ${callId})`
        )
        .limit(1)
        .all()
      return rows.length > 0 ? toAgent(rows[0]) : null
    },

    async findRootAgent(sessionId: string): Promise<Agent | null> {
      const rows = db
        .select()
        .from(schema.agents)
        .where(
          and(
            eq(schema.agents.sessionId, sessionId),
            isNull(schema.agents.parentId),
          )
        )
        .orderBy(desc(schema.agents.createdAt))
        .limit(1)
        .all()
      return rows.length > 0 ? toAgent(rows[0]) : null
    },

    async listBySession(sessionId: string): Promise<Agent[]> {
      const rows = db
        .select()
        .from(schema.agents)
        .where(eq(schema.agents.sessionId, sessionId))
        .orderBy(asc(schema.agents.createdAt))
        .all()
      return rows.map(toAgent)
    },

    async listByParent(parentId: string): Promise<Agent[]> {
      const rows = db
        .select()
        .from(schema.agents)
        .where(eq(schema.agents.parentId, parentId))
        .orderBy(asc(schema.agents.createdAt))
        .all()
      return rows.map(toAgent)
    },
  }
}

function createItemRepo(db: DrizzleInstance): ItemRepository {
  return {
    async create(input: CreateItemInput): Promise<Item> {
      const now = Date.now()
      const id = uuid()

      // Atomic: SELECT MAX(sequence) + INSERT in a single transaction to avoid
      // duplicate sequence numbers when concurrent awaits interleave on the event loop.
      const row = db.transaction((tx) => {
        const maxSeqResult = tx
          .select({ maxSeq: max(schema.items.sequence) })
          .from(schema.items)
          .where(eq(schema.items.agentId, input.agentId))
          .all()
        const nextSequence = (maxSeqResult[0]?.maxSeq ?? -1) + 1

        const insertRow = {
          id,
          agentId: input.agentId,
          sequence: nextSequence,
          type: input.type,
          role: input.role ?? null,
          content: input.content ?? null,
          callId: input.callId ?? null,
          name: input.name ?? null,
          arguments: input.arguments ?? null,
          output: input.output ?? null,
          isError: input.isError != null ? (input.isError ? 1 : 0) : null,
          saveOutput: input.saveOutput != null ? (input.saveOutput ? 1 : 0) : null,
          turnNumber: input.turnNumber,
          durationMs: input.durationMs ?? null,
          createdAt: now,
        }
        tx.insert(schema.items).values(insertRow).run()
        return insertRow
      })

      return toItem(row)
    },

    async listByAgent(agentId: string): Promise<Item[]> {
      const rows = db
        .select()
        .from(schema.items)
        .where(eq(schema.items.agentId, agentId))
        .orderBy(asc(schema.items.sequence))
        .all()
      return rows.map(toItem)
    },

    async listBySession(sessionId: string): Promise<Item[]> {
      // Load items from all root agents (depth=0, no parentId) in the session,
      // ordered by agent creation time then item sequence — gives full conversation history.
      const rows = db
        .select({ item: schema.items })
        .from(schema.items)
        .innerJoin(schema.agents, eq(schema.items.agentId, schema.agents.id))
        .where(
          and(
            eq(schema.agents.sessionId, sessionId),
            isNull(schema.agents.parentId),
          )
        )
        .orderBy(asc(schema.agents.createdAt), asc(schema.items.sequence))
        .all()
      return rows.map((r) => toItem(r.item))
    },

    async getOutputByCallId(callId: string): Promise<Item | null> {
      const rows = db
        .select()
        .from(schema.items)
        .where(
          and(eq(schema.items.callId, callId), eq(schema.items.type, 'function_call_output'))
        )
        .limit(1)
        .all()
      return rows.length > 0 ? toItem(rows[0]) : null
    },
  }
}

function createToolOutputRepo(db: DrizzleInstance): ToolOutputRepository {
  return {
    async save(input: SaveToolOutputInput): Promise<ToolOutput> {
      const now = Date.now()
      const id = uuid()
      const row = {
        id,
        agentId: input.agentId,
        callId: input.callId,
        toolName: input.toolName,
        data: JSON.stringify(input.data),
        createdAt: now,
      }
      db.insert(schema.toolOutputs).values(row).run()
      return toToolOutput(row)
    },

    async getById(id: string): Promise<ToolOutput | null> {
      const rows = db
        .select()
        .from(schema.toolOutputs)
        .where(eq(schema.toolOutputs.id, id))
        .limit(1)
        .all()
      return rows.length > 0 ? toToolOutput(rows[0]) : null
    },

    async listByAgent(agentId: string): Promise<ToolOutput[]> {
      const rows = db
        .select()
        .from(schema.toolOutputs)
        .where(eq(schema.toolOutputs.agentId, agentId))
        .orderBy(asc(schema.toolOutputs.createdAt))
        .all()
      return rows.map(toToolOutput)
    },

    async getLastId(agentId: string): Promise<string | undefined> {
      const rows = db
        .select({ id: schema.toolOutputs.id })
        .from(schema.toolOutputs)
        .where(eq(schema.toolOutputs.agentId, agentId))
        .orderBy(desc(schema.toolOutputs.createdAt))
        .limit(1)
        .all()
      return rows[0]?.id
    },
  }
}

function createModelRepo(db: DrizzleInstance): ModelRepository {
  return {
    async create(input: CreateModelInput): Promise<ModelRecord> {
      const now = Date.now()
      const id = input.id ?? uuid()
      const row = {
        id,
        provider: input.provider,
        modelName: input.name,
        enabled: 1,
        createdAt: now,
      }
      db.insert(schema.models).values(row).run()
      return toModelRecord(row)
    },

    async list(): Promise<ModelRecord[]> {
      const rows = db.select().from(schema.models).orderBy(asc(schema.models.createdAt)).all()
      return rows.map(toModelRecord)
    },

    async getById(id: string): Promise<ModelRecord | null> {
      const rows = db.select().from(schema.models).where(eq(schema.models.id, id)).limit(1).all()
      return rows.length > 0 ? toModelRecord(rows[0]) : null
    },

    async update(id: string, input: UpdateModelInput): Promise<ModelRecord> {
      const updates: Record<string, unknown> = {}
      // The schema doesn't have displayName/maxTokens/contextWindow columns,
      // so we only update modelName if displayName is provided as a fallback
      if (input.displayName !== undefined) updates.modelName = input.displayName

      if (Object.keys(updates).length > 0) {
        db.update(schema.models).set(updates).where(eq(schema.models.id, id)).run()
      }

      const rows = db.select().from(schema.models).where(eq(schema.models.id, id)).limit(1).all()
      if (rows.length === 0) throw new Error(`Model not found: ${id}`)
      return toModelRecord(rows[0])
    },

    async delete(id: string): Promise<void> {
      db.delete(schema.models).where(eq(schema.models.id, id)).run()
    },
  }
}

function createApiKeyRepo(db: DrizzleInstance, encKey: string | null): ApiKeyRepository {
  return {
    async getByProvider(provider: string): Promise<ApiKeyRecord | null> {
      const rows = db
        .select()
        .from(schema.apiKeys)
        .where(eq(schema.apiKeys.provider, provider))
        .limit(1)
        .all()
      if (rows.length === 0) return null
      const record = toApiKeyRecord(rows[0])
      // Decrypt the stored key (gracefully handles legacy plaintext)
      record.encryptedKey = encKey ? decrypt(record.encryptedKey, encKey) : record.encryptedKey
      return record
    },

    async upsert(provider: string, plaintextKey: string): Promise<ApiKeyRecord> {
      const now = Date.now()
      const storedValue = encKey ? encrypt(plaintextKey, encKey) : plaintextKey

      const existing = db
        .select()
        .from(schema.apiKeys)
        .where(eq(schema.apiKeys.provider, provider))
        .limit(1)
        .all()

      if (existing.length > 0) {
        db.update(schema.apiKeys)
          .set({ apiKey: storedValue })
          .where(eq(schema.apiKeys.provider, provider))
          .run()
        return {
          provider,
          encryptedKey: plaintextKey, // return plaintext to caller
          createdAt: existing[0].createdAt,
          updatedAt: now,
        }
      }

      const id = uuid()
      const row = {
        id,
        provider,
        apiKey: storedValue,
        createdAt: now,
      }
      db.insert(schema.apiKeys).values(row).run()
      return {
        provider,
        encryptedKey: plaintextKey, // return plaintext to caller
        createdAt: now,
        updatedAt: now,
      }
    },

    async delete(provider: string): Promise<void> {
      db.delete(schema.apiKeys).where(eq(schema.apiKeys.provider, provider)).run()
    },
  }
}

function createSystemPromptRepo(db: DrizzleInstance): SystemPromptRepository {
  return {
    async create(input: CreateSystemPromptInput): Promise<SystemPromptRecord> {
      const now = Date.now()
      const id = uuid()
      const row = {
        id,
        name: input.name,
        content: input.content,
        createdAt: now,
        updatedAt: now,
      }
      db.insert(schema.systemPrompts).values(row).run()
      return toSystemPromptRecord(row)
    },

    async list(): Promise<SystemPromptRecord[]> {
      const rows = db
        .select()
        .from(schema.systemPrompts)
        .orderBy(asc(schema.systemPrompts.createdAt))
        .all()
      return rows.map(toSystemPromptRecord)
    },

    async getById(id: string): Promise<SystemPromptRecord | null> {
      const rows = db
        .select()
        .from(schema.systemPrompts)
        .where(eq(schema.systemPrompts.id, id))
        .limit(1)
        .all()
      return rows.length > 0 ? toSystemPromptRecord(rows[0]) : null
    },

    async update(id: string, input: UpdateSystemPromptInput): Promise<SystemPromptRecord> {
      const now = Date.now()
      const updates: Record<string, unknown> = { updatedAt: now }
      if (input.name !== undefined) updates.name = input.name
      if (input.content !== undefined) updates.content = input.content

      db.update(schema.systemPrompts).set(updates).where(eq(schema.systemPrompts.id, id)).run()

      const rows = db
        .select()
        .from(schema.systemPrompts)
        .where(eq(schema.systemPrompts.id, id))
        .limit(1)
        .all()
      if (rows.length === 0) throw new Error(`System prompt not found: ${id}`)
      return toSystemPromptRecord(rows[0])
    },

    async delete(id: string): Promise<void> {
      db.delete(schema.systemPrompts).where(eq(schema.systemPrompts.id, id)).run()
    },
  }
}

function createPreferenceRepo(db: DrizzleInstance): PreferenceRepository {
  return {
    async get(key: string): Promise<string | null> {
      const rows = db
        .select()
        .from(schema.preferences)
        .where(eq(schema.preferences.key, key))
        .limit(1)
        .all()
      return rows.length > 0 ? rows[0].value : null
    },

    async set(key: string, value: string): Promise<void> {
      db.insert(schema.preferences)
        .values({ key, value })
        .onConflictDoUpdate({
          target: schema.preferences.key,
          set: { value },
        })
        .run()
    },

    async delete(key: string): Promise<void> {
      db.delete(schema.preferences).where(eq(schema.preferences.key, key)).run()
    },
  }
}

// --- Public API ---

export function createDatabase(url: string): DrizzleInstance {
  const sqlite = new Database(url)
  sqlite.pragma('journal_mode = WAL')
  sqlite.pragma('foreign_keys = ON')

  // Auto-create tables if they don't exist
  sqlite.exec(`
    CREATE TABLE IF NOT EXISTS users (
      id TEXT PRIMARY KEY,
      email TEXT UNIQUE,
      api_key_hash TEXT UNIQUE NOT NULL,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS sessions (
      id TEXT PRIMARY KEY,
      user_id TEXT REFERENCES users(id),
      root_agent_id TEXT,
      title TEXT,
      summary TEXT,
      status TEXT DEFAULT 'active',
      created_at INTEGER,
      updated_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS agents (
      id TEXT PRIMARY KEY,
      session_id TEXT REFERENCES sessions(id),
      parent_id TEXT REFERENCES agents(id),
      source_call_id TEXT,
      depth INTEGER DEFAULT 0,
      task TEXT NOT NULL,
      config TEXT,
      status TEXT DEFAULT 'pending',
      waiting_for TEXT DEFAULT '[]',
      result TEXT,
      error TEXT,
      turn_count INTEGER DEFAULT 0,
      plan TEXT,
      created_at INTEGER,
      updated_at INTEGER,
      completed_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS items (
      id TEXT PRIMARY KEY,
      agent_id TEXT REFERENCES agents(id),
      sequence INTEGER NOT NULL,
      type TEXT NOT NULL,
      role TEXT,
      content TEXT,
      call_id TEXT,
      name TEXT,
      arguments TEXT,
      output TEXT,
      is_error INTEGER,
      save_output INTEGER,
      turn_number INTEGER,
      duration_ms INTEGER,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS api_keys (
      id TEXT PRIMARY KEY,
      provider TEXT NOT NULL,
      api_key TEXT NOT NULL,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS models (
      id TEXT PRIMARY KEY,
      provider TEXT NOT NULL,
      model_name TEXT NOT NULL,
      enabled INTEGER DEFAULT 1,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS system_prompts (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      content TEXT NOT NULL,
      created_at INTEGER,
      updated_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS tool_outputs (
      id TEXT PRIMARY KEY,
      agent_id TEXT REFERENCES agents(id),
      call_id TEXT,
      tool_name TEXT NOT NULL,
      data TEXT,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS preferences (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS agents_session_id_idx ON agents(session_id);
    CREATE INDEX IF NOT EXISTS agents_status_idx ON agents(status);
    CREATE INDEX IF NOT EXISTS items_agent_id_sequence_idx ON items(agent_id, sequence);
    CREATE INDEX IF NOT EXISTS items_call_id_idx ON items(call_id);
  `)

  return drizzle(sqlite, { schema })
}

export class SQLiteRepositories {
  users: UserRepository
  sessions: SessionRepository
  agents: AgentRepository
  items: ItemRepository
  toolOutputs: ToolOutputRepository
  models: ModelRepository
  apiKeys: ApiKeyRepository
  systemPrompts: SystemPromptRepository
  preferences: PreferenceRepository

  constructor(db: DrizzleInstance, encryptionKey?: string) {
    const encKey = encryptionKey ? deriveKey(encryptionKey) : null
    this.users = createUserRepo(db)
    this.sessions = createSessionRepo(db)
    this.agents = createAgentRepo(db)
    this.items = createItemRepo(db)
    this.toolOutputs = createToolOutputRepo(db)
    this.models = createModelRepo(db)
    this.apiKeys = createApiKeyRepo(db, encKey)
    this.systemPrompts = createSystemPromptRepo(db)
    this.preferences = createPreferenceRepo(db)
  }
}

export async function seedDevUser(db: DrizzleInstance): Promise<User> {
  const hash = createHash('sha256').update('dev-key').digest('hex')

  const existing = db
    .select()
    .from(schema.users)
    .where(eq(schema.users.apiKeyHash, hash))
    .limit(1)
    .all()

  if (existing.length > 0) {
    return toUser(existing[0])
  }

  const now = Date.now()
  const id = uuid()
  const row = {
    id,
    email: 'dev@localhost',
    apiKeyHash: hash,
    createdAt: now,
    updatedAt: now,
  }
  db.insert(schema.users).values(row).run()
  return toUser(row)
}
