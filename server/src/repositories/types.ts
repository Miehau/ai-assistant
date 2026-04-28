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
} from '../domain/types.js'
import type { WorkflowRun, WorkflowRunStatus } from '../workflows/types.js'

// --- Input types (omit auto-generated fields) ---

export interface CreateAgentInput {
  sessionId: string
  parentId?: string | null
  sourceCallId?: string | null
  depth?: number
  task: string
  config: AgentConfig
}

export interface UpdateAgentInput {
  status?: AgentStatus
  waitingFor?: WaitingFor[]
  result?: string | null
  error?: string | null
  turnCount?: number
  plan?: Plan | null
  completedAt?: number | null
}

export interface CreateSessionInput {
  userId: string
  title?: string | null
  parentSessionId?: string | null
  forkedFromItemId?: string | null
  source?: string | null
}

export interface UpdateSessionInput {
  rootAgentId?: string | null
  title?: string | null
  summary?: string | null
  status?: SessionStatus
  parentSessionId?: string | null
  forkedFromItemId?: string | null
  source?: string | null
}

export interface CreateUserInput {
  email?: string | null
  apiKeyHash: string
}

export interface CreateItemInput {
  agentId: string
  type: ItemType
  role?: ItemRole | null
  content?: string | null
  callId?: string | null
  name?: string | null
  arguments?: string | null
  output?: string | null
  contentBlocks?: ItemContentBlock[] | null
  isError?: boolean | null
  saveOutput?: boolean | null
  turnNumber: number
  durationMs?: number | null
}

export interface SaveToolOutputInput {
  agentId: string
  callId: string
  toolName: string
  data: unknown
}

export interface CreateModelInput {
  id?: string
  provider: string
  name: string
  displayName?: string
  maxTokens?: number
  contextWindow?: number
}

export interface UpdateModelInput {
  displayName?: string
  maxTokens?: number
  contextWindow?: number
}

export interface ModelRecord {
  id: string
  provider: string
  name: string
  displayName: string | null
  maxTokens: number | null
  contextWindow: number | null
  createdAt: number
  updatedAt: number
}

export interface ApiKeyRecord {
  provider: string
  encryptedKey: string
  createdAt: number
  updatedAt: number
}

export interface SystemPromptRecord {
  id: string
  name: string
  content: string
  isDefault: boolean
  createdAt: number
  updatedAt: number
}

export interface CreateSystemPromptInput {
  name: string
  content: string
  isDefault?: boolean
}

export interface UpdateSystemPromptInput {
  name?: string
  content?: string
  isDefault?: boolean
}

// --- Repository interfaces ---

export interface AgentRepository {
  create(input: CreateAgentInput): Promise<Agent>
  getById(id: string): Promise<Agent | null>
  update(id: string, input: UpdateAgentInput): Promise<Agent>
  failRunningOrWaiting(error: string): Promise<void>
  findWaitingForCall(callId: string): Promise<Agent | null>
  findRootAgent(sessionId: string): Promise<Agent | null>
  listBySession(sessionId: string): Promise<Agent[]>
  listByParent(parentId: string): Promise<Agent[]>
}

export interface ItemRepository {
  create(input: CreateItemInput): Promise<Item>
  getById(id: string): Promise<Item | null>
  listByAgent(agentId: string): Promise<Item[]>
  listBySession(sessionId: string): Promise<Item[]>
  getOutputByCallId(callId: string): Promise<Item | null>
}

export interface SessionRepository {
  create(input: CreateSessionInput): Promise<Session>
  getById(id: string): Promise<Session | null>
  listByUser(userId: string): Promise<Session[]>
  update(id: string, input: UpdateSessionInput): Promise<Session>
  delete(id: string): Promise<void>
}

export interface UserRepository {
  create(input: CreateUserInput): Promise<User>
  list(): Promise<User[]>
  getById(id: string): Promise<User | null>
  getByApiKeyHash(apiKeyHash: string): Promise<User | null>
  setApiKeyHash(id: string, apiKeyHash: string): Promise<User>
}

export interface ToolOutputRepository {
  save(input: SaveToolOutputInput): Promise<ToolOutput>
  getById(id: string): Promise<ToolOutput | null>
  listByAgent(agentId: string): Promise<ToolOutput[]>
  getLastId(agentId: string): Promise<string | undefined>
}

export interface ModelRepository {
  create(input: CreateModelInput): Promise<ModelRecord>
  list(): Promise<ModelRecord[]>
  getById(id: string): Promise<ModelRecord | null>
  update(id: string, input: UpdateModelInput): Promise<ModelRecord>
  delete(id: string): Promise<void>
}

export interface ApiKeyRepository {
  getByProvider(provider: string): Promise<ApiKeyRecord | null>
  upsert(provider: string, encryptedKey: string): Promise<ApiKeyRecord>
  delete(provider: string): Promise<void>
}

export interface SystemPromptRepository {
  create(input: CreateSystemPromptInput): Promise<SystemPromptRecord>
  list(): Promise<SystemPromptRecord[]>
  getById(id: string): Promise<SystemPromptRecord | null>
  update(id: string, input: UpdateSystemPromptInput): Promise<SystemPromptRecord>
  delete(id: string): Promise<void>
}

export interface PreferenceRepository {
  get(key: string): Promise<string | null>
  set(key: string, value: string): Promise<void>
  delete(key: string): Promise<void>
}

// --- Workflow runs ---

export interface CreateWorkflowRunInput {
  workflowName: string
  sessionId: string
  triggerAgentId?: string | null
  triggerCallId?: string | null
  input: unknown
}

export interface UpdateWorkflowRunInput {
  status?: WorkflowRunStatus
  output?: unknown | null
  steps?: import('../workflows/types.js').WorkflowStep[]
  error?: string | null
  startedAt?: number | null
  completedAt?: number | null
}

export interface WorkflowRunRepository {
  create(input: CreateWorkflowRunInput): Promise<WorkflowRun>
  getById(id: string): Promise<WorkflowRun | null>
  update(id: string, input: UpdateWorkflowRunInput): Promise<WorkflowRun>
  listBySession(sessionId: string): Promise<WorkflowRun[]>
}

// --- MCP persistence ---

export type StoredMcpTransport = 'stdio' | 'streamable_http'
export type StoredMcpServerStatus = 'disabled' | 'connected' | 'error'

export interface StoredMcpServer {
  id: string
  name: string
  transport: StoredMcpTransport
  command: string | null
  args: string | null
  env: string | null
  cwd: string | null
  url: string | null
  bearerToken: string | null
  enabled: boolean
  status: StoredMcpServerStatus
  error: string | null
  createdAt: number
  updatedAt: number
}

export interface StoredMcpTool {
  id: string
  serverId: string
  remoteName: string
  registeredName: string
  description: string | null
  inputSchema: string | null
  enabledForNewSessions: boolean
  createdAt: number
  updatedAt: number
}

export interface CreateStoredMcpServerInput {
  id: string
  name: string
  transport: StoredMcpTransport
  command: string | null
  args: string
  env: string
  cwd: string | null
  url: string | null
  bearerToken: string | null
  enabled: boolean
  status: StoredMcpServerStatus
  error: string | null
  createdAt: number
  updatedAt: number
}

export interface UpdateStoredMcpServerInput {
  name?: string
  transport?: StoredMcpTransport
  command?: string | null
  args?: string
  env?: string
  cwd?: string | null
  url?: string | null
  bearerToken?: string | null
  enabled?: boolean
  status?: StoredMcpServerStatus
  error?: string | null
  updatedAt?: number
}

export interface CreateStoredMcpToolInput {
  id: string
  serverId: string
  remoteName: string
  registeredName: string
  description: string
  inputSchema: string
  enabledForNewSessions: boolean
  createdAt: number
  updatedAt: number
}

export interface UpdateStoredMcpToolInput {
  registeredName?: string
  description?: string
  inputSchema?: string
  enabledForNewSessions?: boolean
  updatedAt?: number
}

export interface McpRepository {
  listEnabledServers(): Promise<StoredMcpServer[]>
  listServers(): Promise<StoredMcpServer[]>
  getServer(id: string): Promise<StoredMcpServer | null>
  createServer(input: CreateStoredMcpServerInput): Promise<void>
  updateServer(id: string, input: UpdateStoredMcpServerInput): Promise<void>
  deleteServerAndTools(id: string): Promise<void>
  listTools(serverId?: string): Promise<StoredMcpTool[]>
  getTool(id: string): Promise<StoredMcpTool | null>
  getToolByServerAndRemoteName(serverId: string, remoteName: string): Promise<StoredMcpTool | null>
  createTool(input: CreateStoredMcpToolInput): Promise<void>
  updateTool(id: string, input: UpdateStoredMcpToolInput): Promise<void>
  deleteToolsForServer(serverId: string): Promise<void>
}

// --- Telegram persistence ---

export interface StoredTelegramConnection {
  id: string
  userId: string
  botToken: string
  botUsername: string | null
  allowedTelegramUserId: string
  webhookPathSecret: string
  webhookHeaderSecret: string
  webhookUrl: string | null
  status: string
  lastError: string | null
  createdAt: number
  updatedAt: number
}

export interface StoredTelegramMessageLink {
  id: string
  connectionId: string
  telegramChatId: string
  telegramMessageId: number
  sessionId: string
  itemId: string | null
  senderType: string
  createdAt: number
}

export interface CreateStoredTelegramConnectionInput extends StoredTelegramConnection {}

export interface UpdateStoredTelegramConnectionInput {
  botToken?: string
  botUsername?: string | null
  allowedTelegramUserId?: string
  webhookUrl?: string | null
  status?: string
  lastError?: string | null
  updatedAt?: number
}

export interface CreateStoredTelegramMessageLinkInput extends StoredTelegramMessageLink {}

export interface TelegramRepository {
  listConnections(userId: string): Promise<StoredTelegramConnection[]>
  getConnection(id: string, userId?: string): Promise<StoredTelegramConnection | null>
  createConnection(input: CreateStoredTelegramConnectionInput): Promise<void>
  updateConnection(id: string, input: UpdateStoredTelegramConnectionInput): Promise<void>
  deleteConnection(id: string, userId: string): Promise<boolean>
  hasProcessedUpdate(connectionId: string, updateId: number): Promise<boolean>
  createUpdateDedupe(connectionId: string, updateId: number): Promise<void>
  getMessageLink(connectionId: string, chatId: string, messageId: number): Promise<StoredTelegramMessageLink | null>
  getSessionHeadLink(connectionId: string, sessionId: string): Promise<StoredTelegramMessageLink | null>
  createMessageLink(input: CreateStoredTelegramMessageLinkInput): Promise<void>
}
