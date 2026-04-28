import postgres from 'postgres'
import { drizzle, type PostgresJsDatabase } from 'drizzle-orm/postgres-js'
import { and, asc, desc, eq, isNull, max, or, sql } from 'drizzle-orm'
import { v4 as uuid } from 'uuid'
import { encrypt, decrypt, deriveKey } from '../../lib/crypto.js'
import * as schema from '../../db/schema-pg.js'
import type {
  AgentRepository,
  ApiKeyRecord,
  ApiKeyRepository,
  CreateAgentInput,
  CreateItemInput,
  CreateModelInput,
  CreateSessionInput,
  CreateSystemPromptInput,
  CreateUserInput,
  CreateWorkflowRunInput,
  ItemRepository,
  McpRepository,
  ModelRecord,
  ModelRepository,
  PreferenceRepository,
  SaveToolOutputInput,
  SessionRepository,
  StoredMcpServer,
  StoredMcpTool,
  StoredTelegramConnection,
  StoredTelegramMessageLink,
  SystemPromptRecord,
  SystemPromptRepository,
  TelegramRepository,
  ToolOutputRepository,
  UpdateAgentInput,
  UpdateModelInput,
  UpdateSessionInput,
  UpdateSystemPromptInput,
  UpdateWorkflowRunInput,
  UserRepository,
  WorkflowRunRepository,
} from '../types.js'
import type {
  Agent,
  AgentConfig,
  AgentStatus,
  Item,
  ItemContentBlock,
  ItemRole,
  ItemType,
  Plan,
  Session,
  SessionStatus,
  ToolOutput,
  User,
  WaitingFor,
} from '../../domain/types.js'
import type { WorkflowRun, WorkflowRunStatus } from '../../workflows/types.js'

export type PgDrizzleInstance = PostgresJsDatabase<typeof schema>
type PgClient = postgres.Sql

function boolValue(value: boolean | number | null | undefined): boolean {
  return value === true || value === 1
}

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
    parentSessionId: row.parentSessionId ?? null,
    forkedFromItemId: row.forkedFromItemId ?? null,
    source: row.source ?? null,
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
    contentBlocks: row.contentBlocks ? (JSON.parse(row.contentBlocks) as ItemContentBlock[]) : null,
    isError: row.isError != null ? boolValue(row.isError) : null,
    saveOutput: row.saveOutput != null ? boolValue(row.saveOutput) : null,
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

function toStoredMcpServer(row: typeof schema.mcpServers.$inferSelect): StoredMcpServer {
  return {
    id: row.id,
    name: row.name,
    transport: row.transport as StoredMcpServer['transport'],
    command: row.command ?? null,
    args: row.args ?? null,
    env: row.env ?? null,
    cwd: row.cwd ?? null,
    url: row.url ?? null,
    bearerToken: row.bearerToken ?? null,
    enabled: boolValue(row.enabled),
    status: row.status as StoredMcpServer['status'],
    error: row.error ?? null,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  }
}

function toStoredMcpTool(row: typeof schema.mcpTools.$inferSelect): StoredMcpTool {
  return {
    id: row.id,
    serverId: row.serverId,
    remoteName: row.remoteName,
    registeredName: row.registeredName,
    description: row.description ?? null,
    inputSchema: row.inputSchema ?? null,
    enabledForNewSessions: boolValue(row.enabledForNewSessions),
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  }
}

function toStoredTelegramConnection(row: typeof schema.telegramConnections.$inferSelect): StoredTelegramConnection {
  return {
    id: row.id,
    userId: row.userId,
    botToken: row.botToken,
    botUsername: row.botUsername ?? null,
    allowedTelegramUserId: row.allowedTelegramUserId,
    webhookPathSecret: row.webhookPathSecret,
    webhookHeaderSecret: row.webhookHeaderSecret,
    webhookUrl: row.webhookUrl ?? null,
    status: row.status,
    lastError: row.lastError ?? null,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  }
}

function toStoredTelegramMessageLink(row: typeof schema.telegramMessageLinks.$inferSelect): StoredTelegramMessageLink {
  return {
    id: row.id,
    connectionId: row.connectionId,
    telegramChatId: row.telegramChatId,
    telegramMessageId: row.telegramMessageId,
    sessionId: row.sessionId,
    itemId: row.itemId ?? null,
    senderType: row.senderType,
    createdAt: row.createdAt,
  }
}

function createUserRepo(db: PgDrizzleInstance): UserRepository {
  return {
    async create(input: CreateUserInput): Promise<User> {
      const now = Date.now()
      const row = {
        id: uuid(),
        email: input.email ?? null,
        apiKeyHash: input.apiKeyHash,
        createdAt: now,
        updatedAt: now,
      }
      await db.insert(schema.users).values(row)
      return toUser(row)
    },

    async list(): Promise<User[]> {
      const rows = await db.select().from(schema.users).orderBy(asc(schema.users.createdAt))
      return rows.map(toUser)
    },

    async getById(id: string): Promise<User | null> {
      const rows = await db.select().from(schema.users).where(eq(schema.users.id, id)).limit(1)
      return rows[0] ? toUser(rows[0]) : null
    },

    async getByApiKeyHash(apiKeyHash: string): Promise<User | null> {
      const rows = await db.select().from(schema.users).where(eq(schema.users.apiKeyHash, apiKeyHash)).limit(1)
      return rows[0] ? toUser(rows[0]) : null
    },

    async setApiKeyHash(id: string, apiKeyHash: string): Promise<User> {
      await db.update(schema.users)
        .set({ apiKeyHash, updatedAt: Date.now() })
        .where(eq(schema.users.id, id))
      const row = await this.getById(id)
      if (!row) throw new Error(`User not found: ${id}`)
      return row
    },
  }
}

function createSessionRepo(db: PgDrizzleInstance): SessionRepository {
  return {
    async create(input: CreateSessionInput): Promise<Session> {
      const now = Date.now()
      const row = {
        id: uuid(),
        userId: input.userId,
        rootAgentId: null,
        parentSessionId: input.parentSessionId ?? null,
        forkedFromItemId: input.forkedFromItemId ?? null,
        source: input.source ?? null,
        title: input.title ?? null,
        summary: null,
        status: 'active' as const,
        createdAt: now,
        updatedAt: now,
      }
      await db.insert(schema.sessions).values(row)
      return toSession(row)
    },

    async getById(id: string): Promise<Session | null> {
      const rows = await db.select().from(schema.sessions).where(eq(schema.sessions.id, id)).limit(1)
      return rows[0] ? toSession(rows[0]) : null
    },

    async listByUser(userId: string): Promise<Session[]> {
      const rows = await db
        .select()
        .from(schema.sessions)
        .where(eq(schema.sessions.userId, userId))
        .orderBy(desc(schema.sessions.createdAt))
      return rows.map(toSession)
    },

    async update(id: string, input: UpdateSessionInput): Promise<Session> {
      const updates: Record<string, unknown> = { updatedAt: Date.now() }
      if (input.rootAgentId !== undefined) updates.rootAgentId = input.rootAgentId
      if (input.parentSessionId !== undefined) updates.parentSessionId = input.parentSessionId
      if (input.forkedFromItemId !== undefined) updates.forkedFromItemId = input.forkedFromItemId
      if (input.source !== undefined) updates.source = input.source
      if (input.title !== undefined) updates.title = input.title
      if (input.summary !== undefined) updates.summary = input.summary
      if (input.status !== undefined) updates.status = input.status

      await db.update(schema.sessions).set(updates as any).where(eq(schema.sessions.id, id))
      const rows = await db.select().from(schema.sessions).where(eq(schema.sessions.id, id)).limit(1)
      if (!rows[0]) throw new Error(`Session not found: ${id}`)
      return toSession(rows[0])
    },

    async delete(id: string): Promise<void> {
      await db.transaction(async (tx) => {
        const agentRows = await tx
          .select({ id: schema.agents.id })
          .from(schema.agents)
          .where(eq(schema.agents.sessionId, id))

        for (const agent of agentRows) {
          await tx.delete(schema.items).where(eq(schema.items.agentId, agent.id))
          await tx.delete(schema.toolOutputs).where(eq(schema.toolOutputs.agentId, agent.id))
        }

        await tx.delete(schema.agents).where(eq(schema.agents.sessionId, id))
        await tx.delete(schema.workflowRuns).where(eq(schema.workflowRuns.sessionId, id))
        await tx.delete(schema.telegramMessageLinks).where(eq(schema.telegramMessageLinks.sessionId, id))
        await tx.delete(schema.sessions).where(eq(schema.sessions.id, id))
      })
    },
  }
}

function createAgentRepo(db: PgDrizzleInstance): AgentRepository {
  return {
    async create(input: CreateAgentInput): Promise<Agent> {
      const now = Date.now()
      const row = {
        id: uuid(),
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
      await db.insert(schema.agents).values(row)
      return toAgent(row)
    },

    async getById(id: string): Promise<Agent | null> {
      const rows = await db.select().from(schema.agents).where(eq(schema.agents.id, id)).limit(1)
      return rows[0] ? toAgent(rows[0]) : null
    },

    async update(id: string, input: UpdateAgentInput): Promise<Agent> {
      const updates: Record<string, unknown> = { updatedAt: Date.now() }
      if (input.status !== undefined) updates.status = input.status
      if (input.waitingFor !== undefined) updates.waitingFor = JSON.stringify(input.waitingFor)
      if (input.result !== undefined) updates.result = input.result
      if (input.error !== undefined) updates.error = input.error
      if (input.turnCount !== undefined) updates.turnCount = input.turnCount
      if (input.plan !== undefined) updates.plan = input.plan ? JSON.stringify(input.plan) : null
      if (input.completedAt !== undefined) updates.completedAt = input.completedAt

      await db.update(schema.agents).set(updates as any).where(eq(schema.agents.id, id))
      const rows = await db.select().from(schema.agents).where(eq(schema.agents.id, id)).limit(1)
      if (!rows[0]) throw new Error(`Agent not found: ${id}`)
      return toAgent(rows[0])
    },

    async failRunningOrWaiting(error: string): Promise<void> {
      const now = Date.now()
      await db.update(schema.agents)
        .set({ status: 'failed', error, completedAt: now, updatedAt: now })
        .where(or(eq(schema.agents.status, 'running'), eq(schema.agents.status, 'waiting'))!)
    },

    async findWaitingForCall(callId: string): Promise<Agent | null> {
      const rows = await db
        .select()
        .from(schema.agents)
        .where(sql`EXISTS (SELECT 1 FROM jsonb_array_elements(${schema.agents.waitingFor}::jsonb) elem WHERE elem->>'callId' = ${callId})`)
        .limit(1)
      return rows[0] ? toAgent(rows[0]) : null
    },

    async findRootAgent(sessionId: string): Promise<Agent | null> {
      const rows = await db
        .select()
        .from(schema.agents)
        .where(and(eq(schema.agents.sessionId, sessionId), isNull(schema.agents.parentId)))
        .orderBy(desc(schema.agents.createdAt))
        .limit(1)
      return rows[0] ? toAgent(rows[0]) : null
    },

    async listBySession(sessionId: string): Promise<Agent[]> {
      const rows = await db
        .select()
        .from(schema.agents)
        .where(eq(schema.agents.sessionId, sessionId))
        .orderBy(asc(schema.agents.createdAt))
      return rows.map(toAgent)
    },

    async listByParent(parentId: string): Promise<Agent[]> {
      const rows = await db
        .select()
        .from(schema.agents)
        .where(eq(schema.agents.parentId, parentId))
        .orderBy(asc(schema.agents.createdAt))
      return rows.map(toAgent)
    },
  }
}

function createItemRepo(db: PgDrizzleInstance): ItemRepository {
  return {
    async create(input: CreateItemInput): Promise<Item> {
      const now = Date.now()
      const id = uuid()

      const row = await db.transaction(async (tx) => {
        await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${input.agentId}))`)
        const maxSeqResult = await tx
          .select({ maxSeq: max(schema.items.sequence) })
          .from(schema.items)
          .where(eq(schema.items.agentId, input.agentId))
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
          contentBlocks: input.contentBlocks ? JSON.stringify(input.contentBlocks) : null,
          isError: input.isError ?? null,
          saveOutput: input.saveOutput ?? null,
          turnNumber: input.turnNumber,
          durationMs: input.durationMs ?? null,
          createdAt: now,
        }
        await tx.insert(schema.items).values(insertRow)
        return insertRow
      })

      return toItem(row)
    },

    async getById(id: string): Promise<Item | null> {
      const rows = await db.select().from(schema.items).where(eq(schema.items.id, id)).limit(1)
      return rows[0] ? toItem(rows[0]) : null
    },

    async listByAgent(agentId: string): Promise<Item[]> {
      const rows = await db
        .select()
        .from(schema.items)
        .where(eq(schema.items.agentId, agentId))
        .orderBy(asc(schema.items.sequence))
      return rows.map(toItem)
    },

    async listBySession(sessionId: string): Promise<Item[]> {
      const rows = await db
        .select({ item: schema.items })
        .from(schema.items)
        .innerJoin(schema.agents, eq(schema.items.agentId, schema.agents.id))
        .where(eq(schema.agents.sessionId, sessionId))
        .orderBy(asc(schema.agents.depth), asc(schema.agents.createdAt), asc(schema.items.sequence))
      return rows.map((row) => toItem(row.item))
    },

    async getOutputByCallId(callId: string): Promise<Item | null> {
      const rows = await db
        .select()
        .from(schema.items)
        .where(and(eq(schema.items.callId, callId), eq(schema.items.type, 'function_call_output')))
        .limit(1)
      return rows[0] ? toItem(rows[0]) : null
    },
  }
}

function createToolOutputRepo(db: PgDrizzleInstance): ToolOutputRepository {
  return {
    async save(input: SaveToolOutputInput): Promise<ToolOutput> {
      const row = {
        id: uuid(),
        agentId: input.agentId,
        callId: input.callId,
        toolName: input.toolName,
        data: JSON.stringify(input.data),
        createdAt: Date.now(),
      }
      await db.insert(schema.toolOutputs).values(row)
      return toToolOutput(row)
    },

    async getById(id: string): Promise<ToolOutput | null> {
      const rows = await db.select().from(schema.toolOutputs).where(eq(schema.toolOutputs.id, id)).limit(1)
      return rows[0] ? toToolOutput(rows[0]) : null
    },

    async listByAgent(agentId: string): Promise<ToolOutput[]> {
      const rows = await db
        .select()
        .from(schema.toolOutputs)
        .where(eq(schema.toolOutputs.agentId, agentId))
        .orderBy(asc(schema.toolOutputs.createdAt))
      return rows.map(toToolOutput)
    },

    async getLastId(agentId: string): Promise<string | undefined> {
      const rows = await db
        .select({ id: schema.toolOutputs.id })
        .from(schema.toolOutputs)
        .where(eq(schema.toolOutputs.agentId, agentId))
        .orderBy(desc(schema.toolOutputs.createdAt))
        .limit(1)
      return rows[0]?.id
    },
  }
}

function createModelRepo(db: PgDrizzleInstance): ModelRepository {
  return {
    async create(input: CreateModelInput): Promise<ModelRecord> {
      const row = {
        id: input.id ?? uuid(),
        provider: input.provider,
        modelName: input.name,
        enabled: true,
        createdAt: Date.now(),
      }
      await db.insert(schema.models).values(row)
      return toModelRecord(row)
    },

    async list(): Promise<ModelRecord[]> {
      const rows = await db.select().from(schema.models).orderBy(asc(schema.models.createdAt))
      return rows.map(toModelRecord)
    },

    async getById(id: string): Promise<ModelRecord | null> {
      const rows = await db.select().from(schema.models).where(eq(schema.models.id, id)).limit(1)
      return rows[0] ? toModelRecord(rows[0]) : null
    },

    async update(id: string, input: UpdateModelInput): Promise<ModelRecord> {
      if (input.displayName !== undefined) {
        await db.update(schema.models).set({ modelName: input.displayName }).where(eq(schema.models.id, id))
      }
      const rows = await db.select().from(schema.models).where(eq(schema.models.id, id)).limit(1)
      if (!rows[0]) throw new Error(`Model not found: ${id}`)
      return toModelRecord(rows[0])
    },

    async delete(id: string): Promise<void> {
      await db.delete(schema.models).where(eq(schema.models.id, id))
    },
  }
}

function createApiKeyRepo(db: PgDrizzleInstance, encKey: string | null): ApiKeyRepository {
  return {
    async getByProvider(provider: string): Promise<ApiKeyRecord | null> {
      const rows = await db.select().from(schema.apiKeys).where(eq(schema.apiKeys.provider, provider)).limit(1)
      if (!rows[0]) return null
      const record = toApiKeyRecord(rows[0])
      record.encryptedKey = encKey ? decrypt(record.encryptedKey, encKey) : record.encryptedKey
      return record
    },

    async upsert(provider: string, plaintextKey: string): Promise<ApiKeyRecord> {
      const now = Date.now()
      const storedValue = encKey ? encrypt(plaintextKey, encKey) : plaintextKey
      const existing = await db.select().from(schema.apiKeys).where(eq(schema.apiKeys.provider, provider)).limit(1)

      if (existing[0]) {
        await db.update(schema.apiKeys).set({ apiKey: storedValue }).where(eq(schema.apiKeys.provider, provider))
        return {
          provider,
          encryptedKey: plaintextKey,
          createdAt: existing[0].createdAt,
          updatedAt: now,
        }
      }

      await db.insert(schema.apiKeys).values({
        id: uuid(),
        provider,
        apiKey: storedValue,
        createdAt: now,
      })
      return { provider, encryptedKey: plaintextKey, createdAt: now, updatedAt: now }
    },

    async delete(provider: string): Promise<void> {
      await db.delete(schema.apiKeys).where(eq(schema.apiKeys.provider, provider))
    },
  }
}

function createSystemPromptRepo(db: PgDrizzleInstance): SystemPromptRepository {
  return {
    async create(input: CreateSystemPromptInput): Promise<SystemPromptRecord> {
      const now = Date.now()
      const row = {
        id: uuid(),
        name: input.name,
        content: input.content,
        createdAt: now,
        updatedAt: now,
      }
      await db.insert(schema.systemPrompts).values(row)
      return toSystemPromptRecord(row)
    },

    async list(): Promise<SystemPromptRecord[]> {
      const rows = await db.select().from(schema.systemPrompts).orderBy(asc(schema.systemPrompts.createdAt))
      return rows.map(toSystemPromptRecord)
    },

    async getById(id: string): Promise<SystemPromptRecord | null> {
      const rows = await db.select().from(schema.systemPrompts).where(eq(schema.systemPrompts.id, id)).limit(1)
      return rows[0] ? toSystemPromptRecord(rows[0]) : null
    },

    async update(id: string, input: UpdateSystemPromptInput): Promise<SystemPromptRecord> {
      const updates: Record<string, unknown> = { updatedAt: Date.now() }
      if (input.name !== undefined) updates.name = input.name
      if (input.content !== undefined) updates.content = input.content
      await db.update(schema.systemPrompts).set(updates as any).where(eq(schema.systemPrompts.id, id))
      const rows = await db.select().from(schema.systemPrompts).where(eq(schema.systemPrompts.id, id)).limit(1)
      if (!rows[0]) throw new Error(`System prompt not found: ${id}`)
      return toSystemPromptRecord(rows[0])
    },

    async delete(id: string): Promise<void> {
      await db.delete(schema.systemPrompts).where(eq(schema.systemPrompts.id, id))
    },
  }
}

function createPreferenceRepo(db: PgDrizzleInstance): PreferenceRepository {
  return {
    async get(key: string): Promise<string | null> {
      const rows = await db.select().from(schema.preferences).where(eq(schema.preferences.key, key)).limit(1)
      return rows[0]?.value ?? null
    },

    async set(key: string, value: string): Promise<void> {
      await db.insert(schema.preferences)
        .values({ key, value })
        .onConflictDoUpdate({
          target: schema.preferences.key,
          set: { value },
        })
    },

    async delete(key: string): Promise<void> {
      await db.delete(schema.preferences).where(eq(schema.preferences.key, key))
    },
  }
}

function toWorkflowRun(row: typeof schema.workflowRuns.$inferSelect): WorkflowRun {
  return {
    id: row.id,
    workflowName: row.workflowName,
    sessionId: row.sessionId,
    triggerAgentId: row.triggerAgentId ?? null,
    triggerCallId: row.triggerCallId ?? null,
    status: (row.status ?? 'pending') as WorkflowRunStatus,
    input: row.input ? JSON.parse(row.input) : null,
    output: row.output ? JSON.parse(row.output) : null,
    steps: row.steps ? JSON.parse(row.steps) : [],
    error: row.error ?? null,
    startedAt: row.startedAt ?? null,
    completedAt: row.completedAt ?? null,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  }
}

function createWorkflowRunRepo(db: PgDrizzleInstance): WorkflowRunRepository {
  return {
    async create(input: CreateWorkflowRunInput): Promise<WorkflowRun> {
      const now = Date.now()
      const row = {
        id: uuid(),
        workflowName: input.workflowName,
        sessionId: input.sessionId,
        triggerAgentId: input.triggerAgentId ?? null,
        triggerCallId: input.triggerCallId ?? null,
        status: 'pending' as const,
        input: JSON.stringify(input.input),
        output: null,
        steps: '[]',
        error: null,
        startedAt: null,
        completedAt: null,
        createdAt: now,
        updatedAt: now,
      }
      await db.insert(schema.workflowRuns).values(row)
      return toWorkflowRun(row)
    },

    async getById(id: string): Promise<WorkflowRun | null> {
      const rows = await db.select().from(schema.workflowRuns).where(eq(schema.workflowRuns.id, id)).limit(1)
      return rows[0] ? toWorkflowRun(rows[0]) : null
    },

    async update(id: string, input: UpdateWorkflowRunInput): Promise<WorkflowRun> {
      const updates: Record<string, unknown> = { updatedAt: Date.now() }
      if (input.status !== undefined) updates.status = input.status
      if (input.output !== undefined) updates.output = input.output != null ? JSON.stringify(input.output) : null
      if (input.steps !== undefined) updates.steps = JSON.stringify(input.steps)
      if (input.error !== undefined) updates.error = input.error
      if (input.startedAt !== undefined) updates.startedAt = input.startedAt
      if (input.completedAt !== undefined) updates.completedAt = input.completedAt
      await db.update(schema.workflowRuns).set(updates as any).where(eq(schema.workflowRuns.id, id))
      const rows = await db.select().from(schema.workflowRuns).where(eq(schema.workflowRuns.id, id)).limit(1)
      if (!rows[0]) throw new Error(`WorkflowRun not found: ${id}`)
      return toWorkflowRun(rows[0])
    },

    async listBySession(sessionId: string): Promise<WorkflowRun[]> {
      const rows = await db
        .select()
        .from(schema.workflowRuns)
        .where(eq(schema.workflowRuns.sessionId, sessionId))
        .orderBy(desc(schema.workflowRuns.createdAt))
      return rows.map(toWorkflowRun)
    },
  }
}

function createMcpRepo(db: PgDrizzleInstance): McpRepository {
  return {
    async listEnabledServers(): Promise<StoredMcpServer[]> {
      const rows = await db.select().from(schema.mcpServers).where(eq(schema.mcpServers.enabled, true))
      return rows.map(toStoredMcpServer)
    },

    async listServers(): Promise<StoredMcpServer[]> {
      const rows = await db.select().from(schema.mcpServers).orderBy(asc(schema.mcpServers.name))
      return rows.map(toStoredMcpServer)
    },

    async getServer(id: string): Promise<StoredMcpServer | null> {
      const rows = await db.select().from(schema.mcpServers).where(eq(schema.mcpServers.id, id)).limit(1)
      return rows[0] ? toStoredMcpServer(rows[0]) : null
    },

    async createServer(input): Promise<void> {
      await db.insert(schema.mcpServers).values(input)
    },

    async updateServer(id, input): Promise<void> {
      await db.update(schema.mcpServers).set({ ...input, updatedAt: input.updatedAt ?? Date.now() } as any).where(eq(schema.mcpServers.id, id))
    },

    async deleteServerAndTools(id: string): Promise<void> {
      await db.transaction(async (tx) => {
        await tx.delete(schema.mcpTools).where(eq(schema.mcpTools.serverId, id))
        await tx.delete(schema.mcpServers).where(eq(schema.mcpServers.id, id))
      })
    },

    async listTools(serverId?: string): Promise<StoredMcpTool[]> {
      const rows = serverId
        ? await db.select().from(schema.mcpTools).where(eq(schema.mcpTools.serverId, serverId)).orderBy(asc(schema.mcpTools.remoteName))
        : await db.select().from(schema.mcpTools).orderBy(asc(schema.mcpTools.remoteName))
      return rows.map(toStoredMcpTool)
    },

    async getTool(id: string): Promise<StoredMcpTool | null> {
      const rows = await db.select().from(schema.mcpTools).where(eq(schema.mcpTools.id, id)).limit(1)
      return rows[0] ? toStoredMcpTool(rows[0]) : null
    },

    async getToolByServerAndRemoteName(serverId: string, remoteName: string): Promise<StoredMcpTool | null> {
      const rows = await db
        .select()
        .from(schema.mcpTools)
        .where(and(eq(schema.mcpTools.serverId, serverId), eq(schema.mcpTools.remoteName, remoteName)))
        .limit(1)
      return rows[0] ? toStoredMcpTool(rows[0]) : null
    },

    async createTool(input): Promise<void> {
      await db.insert(schema.mcpTools).values(input)
    },

    async updateTool(id, input): Promise<void> {
      await db.update(schema.mcpTools).set({ ...input, updatedAt: input.updatedAt ?? Date.now() } as any).where(eq(schema.mcpTools.id, id))
    },

    async deleteToolsForServer(serverId: string): Promise<void> {
      await db.delete(schema.mcpTools).where(eq(schema.mcpTools.serverId, serverId))
    },
  }
}

function createTelegramRepo(db: PgDrizzleInstance): TelegramRepository {
  return {
    async listConnections(userId: string): Promise<StoredTelegramConnection[]> {
      const rows = await db
        .select()
        .from(schema.telegramConnections)
        .where(eq(schema.telegramConnections.userId, userId))
        .orderBy(asc(schema.telegramConnections.createdAt))
      return rows.map(toStoredTelegramConnection)
    },

    async getConnection(id: string, userId?: string): Promise<StoredTelegramConnection | null> {
      const where = userId
        ? and(eq(schema.telegramConnections.id, id), eq(schema.telegramConnections.userId, userId))
        : eq(schema.telegramConnections.id, id)
      const rows = await db.select().from(schema.telegramConnections).where(where).limit(1)
      return rows[0] ? toStoredTelegramConnection(rows[0]) : null
    },

    async createConnection(input): Promise<void> {
      await db.insert(schema.telegramConnections).values(input)
    },

    async updateConnection(id, input): Promise<void> {
      await db.update(schema.telegramConnections).set({ ...input, updatedAt: input.updatedAt ?? Date.now() } as any).where(eq(schema.telegramConnections.id, id))
    },

    async deleteConnection(id: string, userId: string): Promise<boolean> {
      const existing = await db
        .select()
        .from(schema.telegramConnections)
        .where(and(eq(schema.telegramConnections.id, id), eq(schema.telegramConnections.userId, userId)))
        .limit(1)
      if (!existing[0]) return false
      await db.transaction(async (tx) => {
        await tx.delete(schema.telegramUpdateDedupe).where(eq(schema.telegramUpdateDedupe.connectionId, id))
        await tx.delete(schema.telegramMessageLinks).where(eq(schema.telegramMessageLinks.connectionId, id))
        await tx.delete(schema.telegramConnections).where(eq(schema.telegramConnections.id, id))
      })
      return true
    },

    async hasProcessedUpdate(connectionId: string, updateId: number): Promise<boolean> {
      const rows = await db
        .select()
        .from(schema.telegramUpdateDedupe)
        .where(and(
          eq(schema.telegramUpdateDedupe.connectionId, connectionId),
          eq(schema.telegramUpdateDedupe.telegramUpdateId, updateId),
        ))
        .limit(1)
      return Boolean(rows[0])
    },

    async createUpdateDedupe(connectionId: string, updateId: number): Promise<void> {
      await db.insert(schema.telegramUpdateDedupe)
        .values({ connectionId, telegramUpdateId: updateId, createdAt: Date.now() })
        .onConflictDoNothing()
    },

    async getMessageLink(connectionId: string, chatId: string, messageId: number): Promise<StoredTelegramMessageLink | null> {
      const rows = await db
        .select()
        .from(schema.telegramMessageLinks)
        .where(and(
          eq(schema.telegramMessageLinks.connectionId, connectionId),
          eq(schema.telegramMessageLinks.telegramChatId, chatId),
          eq(schema.telegramMessageLinks.telegramMessageId, messageId),
        ))
        .limit(1)
      return rows[0] ? toStoredTelegramMessageLink(rows[0]) : null
    },

    async getSessionHeadLink(connectionId: string, sessionId: string): Promise<StoredTelegramMessageLink | null> {
      const rows = await db
        .select()
        .from(schema.telegramMessageLinks)
        .where(and(
          eq(schema.telegramMessageLinks.connectionId, connectionId),
          eq(schema.telegramMessageLinks.sessionId, sessionId),
        ))
        .orderBy(desc(schema.telegramMessageLinks.createdAt), desc(schema.telegramMessageLinks.telegramMessageId))
        .limit(1)
      return rows[0] ? toStoredTelegramMessageLink(rows[0]) : null
    },

    async createMessageLink(input): Promise<void> {
      await db.insert(schema.telegramMessageLinks).values(input)
    },
  }
}

export async function createPgDatabase(url: string): Promise<{ db: PgDrizzleInstance; close: () => Promise<void> }> {
  const client = postgres(url, { max: 10, onnotice: () => {} })
  await ensurePgSchema(client)
  const db = drizzle(client, { schema })
  return { db, close: async () => { await client.end() } }
}

export class PostgresRepositories {
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

  constructor(db: PgDrizzleInstance, encryptionKey?: string) {
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
    this.workflowRuns = createWorkflowRunRepo(db)
    this.mcp = createMcpRepo(db)
    this.telegram = createTelegramRepo(db)
  }
}

async function ensurePgSchema(client: PgClient): Promise<void> {
  await client.unsafe(`
    CREATE TABLE IF NOT EXISTS users (
      id TEXT PRIMARY KEY,
      email TEXT UNIQUE,
      api_key_hash TEXT UNIQUE NOT NULL,
      created_at BIGINT NOT NULL,
      updated_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS sessions (
      id TEXT PRIMARY KEY,
      user_id TEXT,
      root_agent_id TEXT,
      parent_session_id TEXT,
      forked_from_item_id TEXT,
      source TEXT,
      title TEXT,
      summary TEXT,
      status TEXT DEFAULT 'active',
      created_at BIGINT,
      updated_at BIGINT
    );
    CREATE TABLE IF NOT EXISTS agents (
      id TEXT PRIMARY KEY,
      session_id TEXT,
      parent_id TEXT,
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
      created_at BIGINT,
      updated_at BIGINT,
      completed_at BIGINT
    );
    CREATE TABLE IF NOT EXISTS items (
      id TEXT PRIMARY KEY,
      agent_id TEXT,
      sequence INTEGER NOT NULL,
      type TEXT NOT NULL,
      role TEXT,
      content TEXT,
      call_id TEXT,
      name TEXT,
      arguments TEXT,
      output TEXT,
      content_blocks TEXT,
      is_error BOOLEAN,
      save_output BOOLEAN,
      turn_number INTEGER,
      duration_ms INTEGER,
      created_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS api_keys (
      id TEXT PRIMARY KEY,
      provider TEXT NOT NULL UNIQUE,
      api_key TEXT NOT NULL,
      created_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS models (
      id TEXT PRIMARY KEY,
      provider TEXT NOT NULL,
      model_name TEXT NOT NULL,
      enabled BOOLEAN DEFAULT TRUE,
      created_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS system_prompts (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      content TEXT NOT NULL,
      created_at BIGINT,
      updated_at BIGINT
    );
    CREATE TABLE IF NOT EXISTS tool_outputs (
      id TEXT PRIMARY KEY,
      agent_id TEXT,
      call_id TEXT,
      tool_name TEXT NOT NULL,
      data TEXT,
      created_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS preferences (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS workflow_runs (
      id TEXT PRIMARY KEY,
      workflow_name TEXT NOT NULL,
      session_id TEXT NOT NULL,
      trigger_agent_id TEXT,
      trigger_call_id TEXT,
      status TEXT NOT NULL DEFAULT 'pending',
      input TEXT,
      output TEXT,
      steps TEXT,
      error TEXT,
      started_at BIGINT,
      completed_at BIGINT,
      created_at BIGINT NOT NULL,
      updated_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS mcp_servers (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      transport TEXT NOT NULL,
      command TEXT,
      args TEXT,
      env TEXT,
      cwd TEXT,
      url TEXT,
      bearer_token TEXT,
      enabled BOOLEAN NOT NULL DEFAULT FALSE,
      status TEXT NOT NULL DEFAULT 'disabled',
      error TEXT,
      created_at BIGINT NOT NULL,
      updated_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS mcp_tools (
      id TEXT PRIMARY KEY,
      server_id TEXT NOT NULL,
      remote_name TEXT NOT NULL,
      registered_name TEXT NOT NULL,
      description TEXT,
      input_schema TEXT,
      enabled_for_new_sessions BOOLEAN NOT NULL DEFAULT TRUE,
      created_at BIGINT NOT NULL,
      updated_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS telegram_connections (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL,
      bot_token TEXT NOT NULL,
      bot_username TEXT,
      allowed_telegram_user_id TEXT NOT NULL,
      webhook_path_secret TEXT NOT NULL,
      webhook_header_secret TEXT NOT NULL,
      webhook_url TEXT,
      status TEXT NOT NULL DEFAULT 'disconnected',
      last_error TEXT,
      created_at BIGINT NOT NULL,
      updated_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS telegram_message_links (
      id TEXT PRIMARY KEY,
      connection_id TEXT NOT NULL,
      telegram_chat_id TEXT NOT NULL,
      telegram_message_id BIGINT NOT NULL,
      session_id TEXT NOT NULL,
      item_id TEXT,
      sender_type TEXT NOT NULL,
      created_at BIGINT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS telegram_update_dedupe (
      connection_id TEXT NOT NULL,
      telegram_update_id BIGINT NOT NULL,
      created_at BIGINT NOT NULL,
      PRIMARY KEY (connection_id, telegram_update_id)
    );
    CREATE INDEX IF NOT EXISTS agents_session_id_idx ON agents(session_id);
    CREATE INDEX IF NOT EXISTS agents_status_idx ON agents(status);
    CREATE INDEX IF NOT EXISTS items_agent_id_sequence_idx ON items(agent_id, sequence);
    CREATE INDEX IF NOT EXISTS items_call_id_idx ON items(call_id);
    CREATE INDEX IF NOT EXISTS workflow_runs_session_id_idx ON workflow_runs(session_id);
    CREATE INDEX IF NOT EXISTS workflow_runs_status_idx ON workflow_runs(status);
    CREATE INDEX IF NOT EXISTS mcp_servers_name_idx ON mcp_servers(name);
    CREATE INDEX IF NOT EXISTS mcp_servers_enabled_idx ON mcp_servers(enabled);
    CREATE INDEX IF NOT EXISTS mcp_tools_server_id_idx ON mcp_tools(server_id);
    CREATE INDEX IF NOT EXISTS mcp_tools_registered_name_idx ON mcp_tools(registered_name);
    CREATE INDEX IF NOT EXISTS telegram_connections_user_id_idx ON telegram_connections(user_id);
    CREATE INDEX IF NOT EXISTS telegram_connections_status_idx ON telegram_connections(status);
    CREATE INDEX IF NOT EXISTS telegram_message_links_session_id_idx ON telegram_message_links(session_id);
    CREATE INDEX IF NOT EXISTS telegram_message_links_connection_chat_message_idx
      ON telegram_message_links(connection_id, telegram_chat_id, telegram_message_id);
  `)
}
