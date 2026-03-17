import { sqliteTable, text, integer, index } from 'drizzle-orm/sqlite-core'

export const users = sqliteTable('users', {
  id: text('id').primaryKey(),
  email: text('email').unique(),
  apiKeyHash: text('api_key_hash').unique().notNull(),
  createdAt: integer('created_at').notNull(),
  updatedAt: integer('updated_at').notNull(),
})

export const sessions = sqliteTable('sessions', {
  id: text('id').primaryKey(),
  userId: text('user_id').references(() => users.id),
  rootAgentId: text('root_agent_id'),
  title: text('title'),
  summary: text('summary'),
  status: text('status').default('active'),
  createdAt: integer('created_at'),
  updatedAt: integer('updated_at'),
})

export const agents = sqliteTable(
  'agents',
  {
    id: text('id').primaryKey(),
    sessionId: text('session_id').references(() => sessions.id),
    parentId: text('parent_id').references((): any => agents.id),
    sourceCallId: text('source_call_id'),
    depth: integer('depth').default(0),
    task: text('task').notNull(),
    config: text('config'),
    status: text('status').default('pending'),
    waitingFor: text('waiting_for').default('[]'),
    result: text('result'),
    error: text('error'),
    turnCount: integer('turn_count').default(0),
    plan: text('plan'),
    createdAt: integer('created_at'),
    updatedAt: integer('updated_at'),
    completedAt: integer('completed_at'),
  },
  (table) => [
    index('agents_session_id_idx').on(table.sessionId),
    index('agents_status_idx').on(table.status),
  ]
)

export const items = sqliteTable(
  'items',
  {
    id: text('id').primaryKey(),
    agentId: text('agent_id').references(() => agents.id),
    sequence: integer('sequence').notNull(),
    type: text('type').notNull(),
    role: text('role'),
    content: text('content'),
    callId: text('call_id'),
    name: text('name'),
    arguments: text('arguments'),
    output: text('output'),
    contentBlocks: text('content_blocks'),
    isError: integer('is_error'),
    saveOutput: integer('save_output'),
    turnNumber: integer('turn_number'),
    durationMs: integer('duration_ms'),
    createdAt: integer('created_at').notNull(),
  },
  (table) => [
    index('items_agent_id_sequence_idx').on(table.agentId, table.sequence),
    index('items_call_id_idx').on(table.callId),
  ]
)

export const apiKeys = sqliteTable('api_keys', {
  id: text('id').primaryKey(),
  provider: text('provider').notNull(),
  apiKey: text('api_key').notNull(),
  createdAt: integer('created_at').notNull(),
})

export const models = sqliteTable('models', {
  id: text('id').primaryKey(),
  provider: text('provider').notNull(),
  modelName: text('model_name').notNull(),
  enabled: integer('enabled').default(1),
  createdAt: integer('created_at').notNull(),
})

export const systemPrompts = sqliteTable('system_prompts', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  content: text('content').notNull(),
  createdAt: integer('created_at'),
  updatedAt: integer('updated_at'),
})

export const toolOutputs = sqliteTable('tool_outputs', {
  id: text('id').primaryKey(),
  agentId: text('agent_id').references(() => agents.id),
  callId: text('call_id'),
  toolName: text('tool_name').notNull(),
  data: text('data'),
  createdAt: integer('created_at').notNull(),
})

export const preferences = sqliteTable('preferences', {
  key: text('key').primaryKey(),
  value: text('value').notNull(),
})
