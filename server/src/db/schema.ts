import { sqliteTable, text, integer, index, primaryKey } from 'drizzle-orm/sqlite-core'

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
  parentSessionId: text('parent_session_id').references((): any => sessions.id),
  forkedFromItemId: text('forked_from_item_id').references((): any => items.id),
  source: text('source'),
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

export const workflowRuns = sqliteTable(
  'workflow_runs',
  {
    id: text('id').primaryKey(),
    workflowName: text('workflow_name').notNull(),
    sessionId: text('session_id').references(() => sessions.id).notNull(),
    triggerAgentId: text('trigger_agent_id').references(() => agents.id),
    triggerCallId: text('trigger_call_id'),
    status: text('status').default('pending').notNull(),
    input: text('input'),
    output: text('output'),
    steps: text('steps'),
    error: text('error'),
    startedAt: integer('started_at'),
    completedAt: integer('completed_at'),
    createdAt: integer('created_at').notNull(),
    updatedAt: integer('updated_at').notNull(),
  },
  (table) => [
    index('workflow_runs_session_id_idx').on(table.sessionId),
    index('workflow_runs_status_idx').on(table.status),
  ]
)

export const mcpServers = sqliteTable(
  'mcp_servers',
  {
    id: text('id').primaryKey(),
    name: text('name').notNull(),
    transport: text('transport').notNull(),
    command: text('command'),
    args: text('args'),
    env: text('env'),
    cwd: text('cwd'),
    url: text('url'),
    bearerToken: text('bearer_token'),
    enabled: integer('enabled').default(0).notNull(),
    status: text('status').default('disabled').notNull(),
    error: text('error'),
    createdAt: integer('created_at').notNull(),
    updatedAt: integer('updated_at').notNull(),
  },
  (table) => [
    index('mcp_servers_name_idx').on(table.name),
    index('mcp_servers_enabled_idx').on(table.enabled),
  ]
)

export const mcpTools = sqliteTable(
  'mcp_tools',
  {
    id: text('id').primaryKey(),
    serverId: text('server_id').references(() => mcpServers.id).notNull(),
    remoteName: text('remote_name').notNull(),
    registeredName: text('registered_name').notNull(),
    description: text('description'),
    inputSchema: text('input_schema'),
    enabledForNewSessions: integer('enabled_for_new_sessions').default(1).notNull(),
    createdAt: integer('created_at').notNull(),
    updatedAt: integer('updated_at').notNull(),
  },
  (table) => [
    index('mcp_tools_server_id_idx').on(table.serverId),
    index('mcp_tools_registered_name_idx').on(table.registeredName),
  ]
)

export const telegramConnections = sqliteTable(
  'telegram_connections',
  {
    id: text('id').primaryKey(),
    userId: text('user_id').references(() => users.id).notNull(),
    botToken: text('bot_token').notNull(),
    botUsername: text('bot_username'),
    allowedTelegramUserId: text('allowed_telegram_user_id').notNull(),
    webhookPathSecret: text('webhook_path_secret').notNull(),
    webhookHeaderSecret: text('webhook_header_secret').notNull(),
    webhookUrl: text('webhook_url'),
    status: text('status').default('disconnected').notNull(),
    lastError: text('last_error'),
    createdAt: integer('created_at').notNull(),
    updatedAt: integer('updated_at').notNull(),
  },
  (table) => [
    index('telegram_connections_user_id_idx').on(table.userId),
    index('telegram_connections_status_idx').on(table.status),
  ]
)

export const telegramMessageLinks = sqliteTable(
  'telegram_message_links',
  {
    id: text('id').primaryKey(),
    connectionId: text('connection_id').references(() => telegramConnections.id).notNull(),
    telegramChatId: text('telegram_chat_id').notNull(),
    telegramMessageId: integer('telegram_message_id').notNull(),
    sessionId: text('session_id').references(() => sessions.id).notNull(),
    itemId: text('item_id').references(() => items.id),
    senderType: text('sender_type').notNull(),
    createdAt: integer('created_at').notNull(),
  },
  (table) => [
    index('telegram_message_links_session_id_idx').on(table.sessionId),
    index('telegram_message_links_connection_chat_message_idx').on(
      table.connectionId,
      table.telegramChatId,
      table.telegramMessageId,
    ),
  ]
)

export const telegramUpdateDedupe = sqliteTable(
  'telegram_update_dedupe',
  {
    connectionId: text('connection_id').references(() => telegramConnections.id).notNull(),
    telegramUpdateId: integer('telegram_update_id').notNull(),
    createdAt: integer('created_at').notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.connectionId, table.telegramUpdateId] }),
  ]
)
