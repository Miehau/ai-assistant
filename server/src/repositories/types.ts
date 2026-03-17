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
}

export interface UpdateSessionInput {
  rootAgentId?: string | null
  title?: string | null
  summary?: string | null
  status?: SessionStatus
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
  findWaitingForCall(callId: string): Promise<Agent | null>
  findRootAgent(sessionId: string): Promise<Agent | null>
  listBySession(sessionId: string): Promise<Agent[]>
  listByParent(parentId: string): Promise<Agent[]>
}

export interface ItemRepository {
  create(input: CreateItemInput): Promise<Item>
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
  getById(id: string): Promise<User | null>
  getByApiKeyHash(apiKeyHash: string): Promise<User | null>
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
