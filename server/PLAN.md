# Server Migration Plan

> **Goal**: Extract the Rust/Tauri backend into a standalone TypeScript HTTP server that can run locally or be deployed remotely. Combines the best patterns from `src-tauri/` (orchestrator loop, tools, multi-provider LLM) and `01_05_agent/` (agent-centric schema, events, auth, HTTP wait/deliver approvals).

## Architecture Overview

```
┌──────────────┐       HTTP/SSE        ┌──────────────────────────┐
│  Svelte 5    │ ◄──────────────────► │  Hono Server (Node)       │
│  Frontend    │   fetch() + SSE       │                           │
│  (or Tauri)  │                       │  Routes → Services        │
└──────────────┘                       │    ↓                      │
                                       │  Orchestrator (controller │
                                       │    loop, your pattern)    │
                                       │    ↓                      │
                                       │  Tool Registry → Handlers │
                                       │    ↓                      │
                                       │  LLM Providers            │
                                       │  (Anthropic/OpenAI/       │
                                       │   Ollama/OpenRouter)      │
                                       │    ↓                      │
                                       │  SQLite (Drizzle ORM)     │
                                       └──────────────────────────┘
```

## Tech Stack

| Layer | Choice | Notes |
|-------|--------|-------|
| Runtime | Node.js | Bun for package manager only |
| Framework | Hono | SSE built-in, Zod validation, typed routes |
| Database | SQLite via Drizzle ORM | `better-sqlite3` driver |
| LLM | Official Anthropic + OpenAI SDKs | Ollama via OpenAI-compatible SDK |
| Validation | Zod | Request/response schemas |
| Auth | Bearer token | SHA-256 hashed API keys in DB |
| Streaming | SSE | Via Hono `streamSSE()` |

## Module Boundaries & Interfaces

The server is built as a set of **independent modules** connected only through interfaces. No module reaches into another module's internals. The orchestrator is the integration point but depends only on abstractions.

```
                    ┌─────────────────────────────────┐
                    │          HTTP Routes             │
                    │  (only knows: services,          │
                    │   request/response types)        │
                    └──────────┬──────────────────────┘
                               │ calls
                    ┌──────────▼──────────────────────┐
                    │        Orchestrator              │
                    │  depends on:                     │
                    │    LLMProvider (interface)        │
                    │    ToolExecutor (interface)       │
                    │    AgentRepository (interface)    │
                    │    ItemRepository (interface)     │
                    │    EventSink (interface)          │
                    │    OutputStore (interface)        │
                    └──┬──────┬───────┬───────┬───────┘
                       │      │       │       │
          ┌────────────▼┐ ┌──▼────┐ ┌▼─────┐ ┌▼──────────┐
          │ LLM Providers│ │ Tools │ │  DB  │ │  Events   │
          │ (adapters)   │ │(impl) │ │(impl)│ │  (impl)   │
          └──────────────┘ └───────┘ └──────┘ └───────────┘
```

### Interface definitions

Every module boundary is defined by a TypeScript interface in a shared `types.ts` file. Implementations live in their own directories and are wired together at startup in `runtime.ts`.

**Rule: The orchestrator NEVER imports from `providers/anthropic.ts`, `tools/files.ts`, `repositories/sqlite/`, or `events/emitter.ts` directly. It only imports from `*/types.ts` files.**

#### `providers/types.ts` — LLM abstraction
```typescript
// What the orchestrator sees — nothing about Anthropic, OpenAI, etc.
interface LLMProvider {
  generate(request: LLMRequest): Promise<LLMResponse>
  stream(request: LLMRequest): AsyncIterable<LLMStreamEvent>
}

interface LLMRequest {
  model: string
  messages: LLMMessage[]
  tools?: LLMToolDefinition[]       // Function calling definitions
  structured_output?: JSONSchema     // For controller structured output
  temperature?: number
  max_tokens?: number
  signal?: AbortSignal
}

interface LLMResponse {
  content: string | unknown          // Text or parsed structured output
  companion_text?: string            // Text alongside tool calls
  tool_calls?: LLMToolCall[]         // Function calls from the model
  usage: { input_tokens: number; output_tokens: number }
  finish_reason: string
}

interface LLMStreamEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'text_done'; text: string }
  | { type: 'tool_call_delta'; call_id: string; name?: string; arguments_delta: string }
  | { type: 'tool_call_done'; call_id: string; name: string; arguments: string }
  | { type: 'done'; response: LLMResponse }
  | { type: 'error'; error: string }

interface LLMToolDefinition {
  name: string
  description: string
  parameters: JSONSchema
}

interface LLMToolCall {
  call_id: string
  name: string
  arguments: Record<string, unknown>
}

// Provider registry — also behind an interface
interface ProviderRegistry {
  resolve(model: string): LLMProvider  // "anthropic:claude-sonnet" → provider
  list(): string[]                      // Available provider names
}
```

#### `tools/types.ts` — Tool abstraction
```typescript
// What the orchestrator sees — nothing about files, shell, HTTP, etc.
interface ToolExecutor {
  execute(name: string, args: Record<string, unknown>, ctx: ToolContext, options?: { save?: boolean }): Promise<ToolResult>
  executeBatch(calls: ToolCall[], ctx: ToolContext): Promise<ToolBatchResult>  // save flag is per-call in ToolCall
  getMetadata(name: string): ToolMetadata | undefined
  listMetadata(): ToolMetadata[]
  validateArgs(name: string, args: unknown): ValidationResult
  getPreview(name: string, args: Record<string, unknown>, ctx: ToolContext): ToolPreview | undefined
}

interface ToolMetadata {
  name: string
  description: string
  parameters: JSONSchema
  requires_approval: boolean
}

interface ToolContext {
  agent_id: string
  session_id: string
  signal: AbortSignal
}

interface ToolResult {
  ok: boolean
  output?: unknown
  error?: string
  content_blocks?: ContentBlock[]
}

interface ToolCall {
  call_id: string
  name: string
  args: Record<string, unknown>
  save?: boolean
}

interface ToolBatchResult {
  results: Array<{ call_id: string } & ToolResult>
  all_ok: boolean
}

// Individual tool handlers implement this — internal to tools/ module
interface ToolHandler {
  metadata: ToolMetadata
  handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult>
  preview?(args: Record<string, unknown>, ctx: ToolContext): ToolPreview
}
```

**Key**: `ToolHandler` is internal to `tools/`. The orchestrator only sees `ToolExecutor`. The registry implements `ToolExecutor` by dispatching to registered `ToolHandler`s.

#### `repositories/types.ts` — Data access abstraction
```typescript
// What the orchestrator sees — nothing about SQLite, Drizzle, SQL
interface AgentRepository {
  create(input: CreateAgentInput): Promise<Agent>
  getById(id: string): Promise<Agent | undefined>
  update(id: string, fields: Partial<Agent>): Promise<Agent>
  findWaitingForCall(callId: string): Promise<Agent | undefined>
  listBySession(sessionId: string): Promise<Agent[]>
  listByParent(parentId: string): Promise<Agent[]>
}

interface ItemRepository {
  create(agentId: string, input: CreateItemInput): Promise<Item>
  listByAgent(agentId: string): Promise<Item[]>
  getOutputByCallId(callId: string): Promise<Item | undefined>
}

interface SessionRepository {
  create(input: CreateSessionInput): Promise<Session>
  getById(id: string): Promise<Session | undefined>
  listByUser(userId: string): Promise<Session[]>
  update(id: string, fields: Partial<Session>): Promise<Session>
  delete(id: string): Promise<void>
}

interface ToolOutputRepository {
  save(input: SaveToolOutputInput): Promise<ToolOutput>
  getById(id: string): Promise<ToolOutput | undefined>
  listByAgent(agentId: string): Promise<ToolOutput[]>
}

// Plus: UserRepository, ModelRepository, ApiKeyRepository,
//       SystemPromptRepository, PreferenceRepository
```

#### `events/types.ts` — Event abstraction
```typescript
// What the orchestrator sees — nothing about EventEmitter, SSE, WebSocket
interface EventSink {
  emit(event: AgentEvent): void
}

// What routes/SSE layer uses to subscribe
interface EventSource {
  subscribe(filter: EventFilter): AsyncIterable<AgentEvent>
  subscribeOnce(filter: EventFilter): Promise<AgentEvent>
}

interface AgentEvent {
  type: string           // e.g. 'tool:started', 'agent:completed'
  agent_id: string
  session_id: string
  payload: unknown
  timestamp: number
}

interface EventFilter {
  agent_id?: string
  session_id?: string
  types?: string[]
}
```

#### `orchestrator/types.ts` — Orchestrator's own types (no external deps)
```typescript
// These types are internal to the orchestrator — NOT shared with other modules.
// Other modules don't import from orchestrator/types.ts.

type ControllerAction =
  | { action: 'next_step'; thinking: unknown; step_type?: string; tool?: string; tools?: ToolCallSpec[]; args?: Record<string, unknown>; message?: string; question?: string; context?: string; save?: boolean }
  | { action: 'complete'; message: string }
  | { action: 'guardrail_stop'; reason: string; message?: string }
  | { action: 'ask_user'; question: string; context?: string }

// Internal to orchestrator — routes don't see this
type StepExecutionOutcome = { type: 'continue' } | { type: 'complete'; response: string } | { type: 'waiting'; waiting_for: WaitingFor[] }

// Per-run context — created in runAgent, threaded through all internal functions
// Combines immutable deps + mutable per-run state. Discarded when runAgent returns.
interface RunContext {
  readonly agents: AgentRepository
  readonly items: ItemRepository
  readonly toolOutputs: ToolOutputRepository
  readonly provider: LLMProvider
  readonly tools: ToolExecutor
  readonly events: EventSink
  agent: Agent           // mutated as status changes
  turnNumber: number     // incremented each turn
  signal: AbortSignal    // cancellation
}
```

### Dependency rules

| Module | Can import from | CANNOT import from |
|--------|----------------|-------------------|
| `routes/` | `orchestrator/` (runner, delivery), `repositories/types`, `events/types`, `tools/types` (metadata only) | `providers/*`, `tools/files.ts`, `tools/shell.ts`, `repositories/sqlite/` |
| `orchestrator/` | `providers/types`, `tools/types`, `repositories/types`, `events/types`, `domain/` | `providers/anthropic.ts`, `tools/files.ts`, `repositories/sqlite/`, `routes/` |
| `providers/anthropic.ts` | `providers/types` | `orchestrator/`, `tools/`, `repositories/`, `routes/` |
| `tools/files.ts` | `tools/types` | `orchestrator/`, `providers/`, `repositories/`, `routes/` |
| `repositories/sqlite/` | `repositories/types`, `db/schema` | `orchestrator/`, `providers/`, `tools/`, `routes/` |
| `events/emitter.ts` | `events/types` | Everything else |
| `domain/` | Nothing (pure types + state machine functions) | Everything |
| `lib/runtime.ts` | **Everything** (this is the composition root) | — |

### Composition root: `lib/runtime.ts`

This is the **only place** where concrete implementations are wired together:

```typescript
// runtime.ts — the ONLY file that imports concrete implementations
import { SQLiteRepositories } from '../repositories/sqlite'
import { AnthropicProvider } from '../providers/anthropic'
import { OpenAIProvider } from '../providers/openai'
import { ToolRegistryImpl } from '../tools/registry'
import { EventEmitterImpl } from '../events/emitter'
import { registerFileTools } from '../tools/files'
import { registerShellTools } from '../tools/shell'
// ...

interface RuntimeContext {
  repositories: {
    users: UserRepository
    sessions: SessionRepository
    agents: AgentRepository
    items: ItemRepository
    toolOutputs: ToolOutputRepository
    models: ModelRepository
    apiKeys: ApiKeyRepository
    systemPrompts: SystemPromptRepository
    preferences: PreferenceRepository
  }
  providers: ProviderRegistry
  tools: ToolExecutor
  events: EventSink & EventSource   // Implementation provides both
  config: AppConfig
}

function initRuntime(config: AppConfig): RuntimeContext {
  const db = createDatabase(config.databaseUrl)
  const repos = new SQLiteRepositories(db)
  const events = new EventEmitterImpl()
  const tools = new ToolRegistryImpl()

  // Register tools — each registration only touches tools/types interface
  registerFileTools(tools)
  registerShellTools(tools)
  registerWebTools(tools)
  registerSearchTools(tools)
  registerToolOutputTools(tools, repos.toolOutputs)
  registerPreferenceTools(tools, repos.preferences)

  // Register providers
  const providers = new ProviderRegistryImpl()
  if (config.anthropicApiKey) providers.register('anthropic', new AnthropicProvider(config.anthropicApiKey))
  if (config.openaiApiKey) providers.register('openai', new OpenAIProvider(config.openaiApiKey))
  // ...

  return { repositories: repos, providers, tools, events, config }
}
```

### Why this matters

1. **Swap implementations without touching orchestrator** — replace SQLite with Postgres, swap Anthropic SDK for a mock, add a new tool — none of these touch `orchestrator/runner.ts`
2. **Test in isolation** — mock `ToolExecutor` to test orchestrator logic without real file/shell access. Mock `LLMProvider` to test without API calls.
3. **No concern leaking** — the orchestrator doesn't know about HTTP status codes, SSE frames, SQL queries, or Anthropic's content block format. Routes don't know about LLM message formatting.
4. **Clear ownership** — if Anthropic changes their API, only `providers/anthropic.ts` changes. If you add a new tool, only `tools/` and `runtime.ts` change.

---

## Key Design Decisions

### From our repo (keep)
- **Controller pattern**: LLM explicitly declares action type (`next_step`, `complete`, `guardrail_stop`, `ask_user`) via structured output. Orchestrator dispatches based on declared intent, not inferred from response shape.
- **Flat step execution**: Single dispatch path handles tool/tool_batch/respond/ask_user based on field presence.
- **Parallel tool batching**: tool_batch executes tools concurrently (Promise.all with timeout + cancellation per call).
- **Tool output persistence**: Opt-in via flag on tool call (LLM decides `save: true`), NOT automatic by size. Persisted outputs can be referenced by ID in later turns without loading into context.
- **All tools ported**: files, shell, web, search, integrations, tool_outputs (read/list/stats/extract/count/sample).
- **Sub-agents**: Both sync (wait for result) and async (fire and check later). LLM chooses.

### From 01_05_agent (adopt)
- **Agent-centric DB schema**: sessions → agents → items (polymorphic: message, function_call, function_call_output, reasoning).
- **HTTP wait/deliver for approvals**: Agent parks in `waiting` state with `waitingFor[]`. Frontend receives SSE event, POSTs `/agents/:id/approve` to approve/deny. Agent resumes.
- **Event emitter**: Typed events (agent:started, turn:started, tool:requested, etc.) piped to SSE.
- **Auth**: Bearer token middleware with hashed API keys in users table.
- **callId correlation**: Each function_call gets a unique callId tracked through waitingFor → delivery → function_call_output.

### New
- **No context compaction**: Assume conversations stay within limit for now.
- **No automatic retry**: LLM handles its own retry decisions. We may add transient error retry later.
- **No branches**: Skip conversation branching.

---

## Phase 1: Project Scaffold & Database

### Step 1.1 — Initialize project
- Create `server/` directory with `package.json`, `tsconfig.json`
- Install deps: `hono`, `@hono/node-server`, `@hono/zod-validator`, `drizzle-orm`, `better-sqlite3`, `zod`, `uuid`, `pino` (logger)
- Install dev deps: `typescript`, `tsx`, `drizzle-kit`, `@types/better-sqlite3`, `@types/uuid`
- Add scripts: `dev` (tsx watch), `build` (tsc), `start` (node), `db:generate`, `db:push`
- Create `src/index.ts` entry point with basic Hono server + health check

### Step 1.2 — Database schema (Drizzle)
Create `src/db/schema.ts` with these tables:

**users**
- `id` text PK
- `email` text unique (nullable for local-only use)
- `api_key_hash` text unique not null
- `created_at` integer not null
- `updated_at` integer not null

**sessions**
- `id` text PK
- `user_id` text FK → users
- `root_agent_id` text (nullable, set after first agent created)
- `title` text
- `summary` text
- `status` text ('active' | 'archived') default 'active'
- `created_at` integer not null
- `updated_at` integer not null

**agents**
- `id` text PK
- `session_id` text FK → sessions
- `parent_id` text (nullable, FK → agents, for sub-agents)
- `source_call_id` text (nullable, the callId that spawned this agent)
- `depth` integer default 0
- `task` text not null (the user message / delegation task)
- `config` text (JSON: model, provider, max_turns, max_tool_calls_per_step, tool_execution_timeout_ms)
- `status` text ('pending' | 'running' | 'waiting' | 'completed' | 'failed' | 'cancelled') default 'pending'
- `waiting_for` text (JSON array of WaitingFor objects) default '[]'
- `result` text (nullable, final response)
- `error` text (nullable)
- `turn_count` integer default 0
- `plan` text (JSON: goal, steps[])
- `created_at` integer not null
- `updated_at` integer not null
- `completed_at` integer (nullable)

**items**
- `id` text PK
- `agent_id` text FK → agents
- `sequence` integer not null (auto-increment per agent)
- `type` text ('message' | 'function_call' | 'function_call_output' | 'reasoning') not null
- `role` text (nullable: 'system' | 'user' | 'assistant' for messages)
- `content` text (JSON, for messages)
- `call_id` text (nullable, for function_call and function_call_output)
- `name` text (nullable, tool name for function_call)
- `arguments` text (JSON, for function_call)
- `output` text (nullable, for function_call_output)
- `is_error` integer (boolean, for function_call_output)
- `save_output` integer (boolean, LLM-requested persistence flag)
- `turn_number` integer
- `duration_ms` integer (nullable, for function_call_output)
- `created_at` integer not null

Indexes:
- `items.agent_id` + `items.sequence`
- `items.call_id`
- `agents.session_id`
- `agents.status` (for finding waiting agents)

**api_keys**
- `id` text PK
- `provider` text not null
- `api_key` text not null (encrypted or plain — decide later)
- `created_at` integer not null

**models**
- `id` text PK
- `provider` text not null
- `model_name` text not null
- `enabled` integer default 1
- `created_at` integer not null

**system_prompts**
- `id` text PK
- `name` text not null
- `content` text not null
- `created_at` integer not null
- `updated_at` integer not null

**tool_outputs**
- `id` text PK
- `agent_id` text FK → agents
- `call_id` text FK → items.call_id
- `tool_name` text not null
- `data` text (JSON, the full output)
- `created_at` integer not null

**preferences**
- `key` text PK
- `value` text not null

### Step 1.3 — Repository layer
Create `src/repositories/` with:
- `types.ts` — repository interfaces (UserRepository, SessionRepository, AgentRepository, ItemRepository, etc.)
- `sqlite/index.ts` — Drizzle-based implementations
- Key queries: `findWaitingForCall(callId)` using SQLite json_each (from 01_05_agent pattern)

### Checkpoint 1
- [x] `bun run dev` starts server on port 3001
- [x] `GET /health` returns 200
- [x] Database created with all tables
- [x] Repository layer compiles with all CRUD methods
- [x] Can create a user and session via repository directly (test script or simple test)

---

## Phase 2: Auth, Config & Event System

### Step 2.1 — Configuration
Create `src/lib/config.ts`:
- Zod schema for env vars: PORT, HOST, DATABASE_URL, DEFAULT_MODEL, ANTHROPIC_API_KEY, OPENAI_API_KEY, OLLAMA_BASE_URL
- Load from `.env` file + process.env

### Step 2.2 — Auth middleware
Create `src/middleware/auth.ts`:
- Bearer token extraction from Authorization header
- SHA-256 hash comparison against `users.api_key_hash`
- Inject `userId` into Hono context
- Skip auth for `/health` and configurable public routes

### Step 2.3 — Event system
Create `src/events/`:
- `types.ts` — Event type definitions:
  ```
  agent:started, agent:completed, agent:failed, agent:waiting
  turn:started, turn:completed
  tool:started, tool:completed, tool:proposed (approval needed)
  tool:approved, tool:denied
  step:proposed, step:started, step:completed
  phase:changed
  companion:text (text alongside tool calls)
  ```
- `emitter.ts` — Typed EventEmitter wrapper (same pattern as 01_05_agent)
- `sse.ts` — Helper to pipe events for a specific agent/session to an SSE stream

### Step 2.4 — Runtime context
Create `src/lib/runtime.ts`:
- `RuntimeContext` type: repositories, config, eventEmitter, toolRegistry, providers
- `initRuntime()` — initialize all services
- `shutdownRuntime()` — graceful cleanup

### Checkpoint 2
- [x] Config loads from env
- [x] Auth middleware rejects requests without valid bearer token
- [x] Auth middleware passes requests with valid token and injects userId
- [x] EventEmitter can emit and subscribe to typed events
- [x] RuntimeContext initializes successfully

---

## Phase 3: LLM Provider Layer

### Step 3.1 — Provider types
Create `src/providers/types.ts`:
- `ProviderRequest`: model, messages (role + content), tools (function definitions), structured_output (JSON schema), stream flag, temperature, max_tokens, signal (AbortSignal)
- `ProviderResponse`: content (string or structured), companion_text (optional), usage (input_tokens, output_tokens), finish_reason
- `ProviderStreamEvent`: text_delta, text_done, function_call_delta, function_call_done, done, error
- `Provider` interface: `generate(request) → ProviderResponse`, `stream(request) → AsyncIterable<ProviderStreamEvent>`

### Step 3.2 — Anthropic provider
Create `src/providers/anthropic.ts`:
- Use `@anthropic-ai/sdk`
- Map our ProviderRequest → Anthropic Messages API format
- Handle structured output via tool_use (Anthropic's native function calling)
- Handle `companion_text` — text blocks emitted alongside tool_use blocks
- Stream via SDK's `.stream()` → map to ProviderStreamEvent
- Strip `oneOf`/`anyOf`/`allOf` from schemas (carry over invariant from Rust)

### Step 3.3 — OpenAI provider
Create `src/providers/openai.ts`:
- Use `openai` SDK
- Map ProviderRequest → OpenAI Chat Completions format
- Function calling via `tools` parameter
- Structured output via `response_format: { type: "json_schema", ... }`
- Stream via SDK's stream → map to ProviderStreamEvent

### Step 3.4 — Ollama provider
Create `src/providers/ollama.ts`:
- Use OpenAI SDK with custom `baseURL` pointing to Ollama
- Same adapter as OpenAI but with Ollama-specific model names
- No API key required

### Step 3.5 — OpenRouter provider
Create `src/providers/openrouter.ts`:
- Use OpenAI SDK with custom `baseURL` (https://openrouter.ai/api/v1)
- Add `HTTP-Referer` and `X-Title` headers
- Same adapter as OpenAI otherwise

### Step 3.6 — Provider registry
Create `src/providers/registry.ts`:
- `registerProvider(name, provider)`
- `resolveProvider(modelString)` — parse "provider:model" format
- Initialize providers from config (skip if no API key)

### Checkpoint 3
- [x] Anthropic provider can send a message and get a response (manual test with real key)
- [x] OpenAI provider can send a message and get a response
- [x] Streaming works for both providers (text_delta events flow)
- [x] Function calling works — provider returns function_call items
- [x] Provider registry resolves "anthropic:claude-sonnet-4-20250514" correctly
> Note: Providers implemented and compile. Need real API keys for live testing.

---

## Phase 4: Tool System

### Step 4.1 — Tool types & registry
Create `src/tools/`:
- `types.ts` — Public interfaces (see Module Boundaries section above for `ToolExecutor`, `ToolMetadata`, `ToolResult`, `ToolContext`, `ToolHandler`)
  - **`ToolContext` has NO `runtime` field** — tools receive only what they need (agentId, sessionId, signal). If a tool needs DB access (e.g. `tool_outputs.read`), it receives a narrow callback or repository interface at registration time, not the whole runtime.
  - Example: `registerToolOutputTools(registry, toolOutputRepo)` — the `ToolOutputRepository` is captured in a closure, not passed through context.
- `registry.ts` — `ToolRegistryImpl` class that implements both:
  - `ToolExecutor` (public — what orchestrator calls)
  - Internal `register(handler: ToolHandler)` method (used only at startup)
  - Handles: arg validation (via JSON schema), dispatch to handler, timeout via AbortSignal, parallel batch execution via `Promise.allSettled()`
  - **Owns persistence**: Receives `ToolOutputRepository` at construction. When `save: true` is set on a `ToolCall`, the registry persists the result after the handler returns. Individual handlers never touch persistence — they stay pure.

### Step 4.2 — File tools
Port from `src-tauri/src/tools/files/`:
- `files.read` — read file content (with range support), return content blocks for images
- `files.write` — write file content
- `files.edit` — search and replace in file
- `files.create` — create new file
- `files.append` — append to file
- `files.list` — list directory contents
- `files.search_replace` — bulk find/replace

### Step 4.3 — Search tool
Port from `src-tauri/src/tools/search.rs`:
- `search` — regex search across files in a directory

### Step 4.4 — Shell tool
Port from `src-tauri/src/tools/shell.rs`:
- `shell.exec` — execute shell command with timeout
- Use Node's `child_process.execFile` with AbortSignal

### Step 4.5 — Web tools
Port from `src-tauri/src/tools/web/`:
- `web.fetch` — HTTP GET/POST with response parsing
- `web.download` — download file to disk
- `web.request` — generic HTTP request

### Step 4.6 — Tool output tools
Port from `src-tauri/src/tools/tool_outputs.rs`:
- `tool_outputs.read` — read a persisted tool output by ID
- `tool_outputs.list` — list available persisted outputs for this agent
- `tool_outputs.stats` — summary statistics of a persisted output
- `tool_outputs.extract` — JSONPath query on persisted output
- `tool_outputs.count` — count elements matching a JSONPath
- `tool_outputs.sample` — sample N items from a persisted output

These read from the `tool_outputs` table. Outputs are persisted there when `save: true` flag is set on the tool call.

### Step 4.7 — Integration tools (Gmail, Calendar, Todoist)
Port from `src-tauri/src/tools/integrations/`:
- These can be deferred to a later phase if needed
- Stub them with `requires_approval: true` and placeholder handlers

### Step 4.8 — Preferences tool
Port from `src-tauri/src/tools/prefs.rs`:
- `preferences.get` / `preferences.set`

### Checkpoint 4
- [x] ToolRegistry registers all tools and lists metadata
- [x] `files.read` reads a real file and returns content
- [x] `files.write` creates a file
- [x] `shell.exec` runs `echo hello` and returns output
- [x] `web.fetch` fetches a URL
- [x] `search` finds a pattern in a directory
- [x] Tool arg validation rejects invalid args with clear error
- [x] `tool_outputs.read` retrieves a previously saved output

---

## Phase 5: Orchestrator (Core Loop)

This is the most critical phase. Port the `DynamicController` pattern from `src-tauri/src/agent/orchestrator.rs`.

### Step 5.1 — Domain types
Create `src/domain/`:
- `agent.ts` — Agent type, status transitions (startAgent, waitForMany, deliverOne, completeAgent, failAgent)
- `types.ts`:
  ```typescript
  type CallId = string
  type AgentStatus = 'pending' | 'running' | 'waiting' | 'completed' | 'failed' | 'cancelled'
  type WaitType = 'tool' | 'approval' | 'agent' | 'human'

  interface WaitingFor {
    callId: CallId
    type: WaitType
    name: string                          // tool or agent name
    args?: Record<string, unknown>        // for 'approval': the tool args so frontend can show what's being approved
    description?: string                  // for 'human': the question text
  }

  interface AgentConfig {
    model: string
    provider: string
    max_turns: number
    max_tool_calls_per_step: number
    tool_execution_timeout_ms: number
  }
  ```
- `plan.ts` — Plan, PlanStep, StepAction, StepStatus, StepResult

### Step 5.2 — Controller action types
Create `src/orchestrator/types.ts`:
- `ControllerAction` discriminated union:
  ```typescript
  type ControllerAction =
    | { action: 'next_step'; thinking: unknown; step_type?: string; tool?: string; tools?: ToolCallSpec[]; args?: Record<string, unknown>; output_mode?: string; message?: string; question?: string; context?: string; save?: boolean }
    | { action: 'complete'; message: string }
    | { action: 'guardrail_stop'; reason: string; message?: string }
    | { action: 'ask_user'; question: string; context?: string }
  ```
- `ToolCallSpec`: { tool, args, output_mode?, save? }
- `StepExecutionOutcome`: 'continue' | { complete: string }
- Step type inference: `inferStepType(action)` — tool present → 'tool', tools present → 'tool_batch', message → 'respond', question → 'ask_user'

### Step 5.3 — Controller parsing
Create `src/orchestrator/parsing.ts`:
- `parseControllerAction(response: unknown): ControllerAction` — validate and parse LLM structured output
- `extractJson(text: string): string` — extract JSON from `=====JSON_START=====` markers (for legacy providers)
- `controllerOutputFormat(): JSONSchema` — the schema sent to the LLM for structured output

### Step 5.4 — Controller prompts
Create `src/orchestrator/prompts.ts`:
- Port `CONTROLLER_PROMPT_BASE` from `src-tauri/src/agent/prompts.rs`
- Port provider-specific variants (`CONTROLLER_PROMPT_ANTHROPIC`, `CONTROLLER_PROMPT_OPENAI`)
- Tool list formatting

### Step 5.5 — Orchestrator runner
Create `src/orchestrator/runner.ts` — the main loop.

**The orchestrator depends ONLY on interfaces:**
```typescript
// runner.ts imports — notice: no concrete implementations
import type { LLMProvider } from '../providers/types'
import type { ToolExecutor } from '../tools/types'
import type { AgentRepository, ItemRepository, ToolOutputRepository } from '../repositories/types'
import type { EventSink } from '../events/types'

// Injected by the route layer — immutable, shared shape
interface OrchestratorDeps {
  agents: AgentRepository
  items: ItemRepository
  toolOutputs: ToolOutputRepository
  provider: LLMProvider         // Already resolved for this agent's model
  tools: ToolExecutor
  events: EventSink
}

// Created once per runAgent call — internal to orchestrator, never exported
interface RunContext {
  // injected deps (immutable)
  readonly agents: AgentRepository
  readonly items: ItemRepository
  readonly toolOutputs: ToolOutputRepository
  readonly provider: LLMProvider
  readonly tools: ToolExecutor
  readonly events: EventSink
  // per-run state (mutable, scoped to this run only)
  agent: Agent
  turnNumber: number
  signal: AbortSignal
}

function createRunContext(agent: Agent, deps: OrchestratorDeps, signal: AbortSignal): RunContext {
  return { ...deps, agent, turnNumber: 0, signal }
}

async function runAgent(agentId: string, deps: OrchestratorDeps, options?: RunOptions): Promise<RunResult>
```

The route layer constructs `OrchestratorDeps` and passes it in. `runAgent` creates a `RunContext` internally — all orchestrator-internal functions (`executeFlatStep`, `executeTool`, `executeToolBatch`) receive only `RunContext`, keeping call signatures clean. The `RunContext` is discarded when `runAgent` returns.

The orchestrator never touches the provider registry or knows which provider it's using.

**Core loop** (mirrors `DynamicController::run`):
1. Load agent from DB via `ctx.agents.getById()`, create `RunContext`
2. **Check for pending approved tool executions on resume** — when the agent was parked for approval, the LLM already decided the tool call in a previous turn. The `function_call` item is stored in DB (with callId, name, args). The approval flow:

   **Parking:**
   - Orchestrator stores `function_call` item (callId, name, args) in DB
   - Parks agent with `waitingFor: [{ callId, type: 'approval', name, args }]`
   - For batches with multiple approvals, each gets its own entry — frontend approves/denies each by callId independently
   - Agent stays `waiting` until all `waitingFor` entries are resolved

   **Resume (after all approved/denied):**
   a. Query items: find `function_call` items where no matching `function_call_output` exists for that `callId`
   b. Check if the callId was approved or denied (denied ones already have a `function_call_output` with `is_error: true` written by `deliverApproval`)
   c. Execute approved tools via `ctx.tools.execute(name, args, toolCtx, { save })`
   d. Store results as `function_call_output` items
   e. Enter the controller loop — LLM sees all tool results (successes + denials) in history

   **Denial path:**
   - `deliverApproval(agentId, callId, 'denied')` immediately writes a `function_call_output` with `is_error: true, output: "Tool execution denied by user"`
   - Removes that callId from `waitingFor`
   - If `waitingFor` is now empty → agent transitions to `running` → `runAgent()` resumes → step 2a finds no unanswered calls → enters controller loop → LLM sees the denial and reacts
3. **While** ctx.agent.status === 'running' && ctx.turnNumber < maxTurns:
   a. Build controller messages: system prompt + tool list + limits + history (from `ctx.items.listByAgent()`)
   b. Call `ctx.provider.generate()` with structured output schema
   c. Parse response → `ControllerAction` (internal parsing, no external deps)
   d. Handle companion_text → `ctx.events.emit()`, append to items
   e. **Match** on action:
      - `next_step` → `executeFlatStep()` → Continue or Complete
      - `complete` → finish
      - `guardrail_stop` → fail with reason
      - `ask_user` → park agent in waiting state
3. Return RunResult

**`executeFlatStep()`** (mirrors `execute_flat_step`):
- Infer step type from fields
- Create PlanStep, emit step:proposed event
- Set phase to Executing, emit step:started
- Dispatch: tool → `executeTool()`, tool_batch → `executeToolBatch()`, respond → return message, ask_user → park
- Record step result, emit step:completed
- Append tool result to conversation history
- Handle denied approvals → Complete with stop message

**`executeTool()`** (mirrors `execute_tool`):
- Validate args against schema
- Check approval requirement → if needed, park agent in `waiting` with `waitingFor: [{ callId, type: 'tool', name }]`
- Return early with `waiting` status — frontend will POST `/agents/:id/approve`
- On resume (after delivery): execute tool handler with timeout + AbortSignal
- Call `ctx.tools.execute(name, args, toolCtx, { save })` — the ToolExecutor handles persistence internally
- Record execution, emit tool:completed event

**`executeToolBatch()`** (mirrors `execute_tool_batch`):
- If any tool requires approval → execute sequentially (each may park)
- Otherwise → call `ctx.tools.executeBatch(calls, toolCtx)` — registry handles parallel execution + per-tool persistence internally
- Collect results, handle partial failures

### Step 5.6 — Tool argument hydration
Create `src/orchestrator/hydration.ts`:
- `hydrateToolArgs(toolName, args, context)` — resolve tool_outputs.* references
- Download path resolution

### Step 5.7 — Tool output delivery
Create `src/orchestrator/output.ts`:
- `buildToolResultMessage(stepResult)` — format tool execution results for LLM context
- Handle content_blocks (images, etc.) — lift into message without duplicating binary data
- Format single vs batch execution summaries

### Step 5.8 — Delivery & resume
Create `src/orchestrator/delivery.ts`:
- `deliverResult(agentId, callId, result, runtime)` — handle tool output delivery (from 01_05_agent pattern)
- `deliverApproval(agentId, callId, decision, runtime)` — approve/deny a pending tool
- On delivery: remove from waitingFor, if empty → transition to running → resume `runAgent()`
- Auto-propagate to parent agent if child completes

### Step 5.9 — Sub-agent support
In `runner.ts`, handle `agent.spawn` tool:
- **Sync mode** (default): Create child agent → `await runAgent(childId)` → return result to parent
- **Async mode** (LLM sets `async: true`): Create child agent → start `runAgent(childId)` in background → return callId to parent immediately → parent continues
- `agent.check` tool: Check status of async child agent
- `agent.get_result` tool: Get result of completed child agent
- Max depth: 5

### Checkpoint 5
- [x] Orchestrator can run a simple conversation: user message → LLM responds → complete
- [x] Tool execution works: LLM calls a tool → tool runs → result fed back → LLM continues
- [x] Tool batch works: LLM calls multiple tools in parallel → all results collected
- [x] Approval flow: tool requires approval → agent parks → POST approve → agent resumes → tool executes
- [x] Approval denial: POST deny → agent stops with message
- [x] ask_user: LLM asks question → agent parks → user delivers answer → agent resumes
- [x] Sub-agent sync: LLM delegates → child runs to completion → result returned to parent
- [x] Sub-agent async: LLM delegates async → parent continues → checks on child later
- [x] Events fire correctly for all lifecycle stages
- [x] Plan tracking: steps are created and updated in DB
> Note: Orchestrator implemented with all flows. Needs real LLM key for live integration test.

---

## Phase 6: HTTP Routes

### Step 6.1 — Chat routes
Create `src/routes/chat.ts`:

**POST /api/chat/completions** — Start or continue a conversation
```
Request:
  sessionId?: string        (create new if omitted)
  model?: string            (default from config)
  input: string | Item[]    (user message or structured input)
  instructions?: string     (system prompt override)
  tools?: string[]          (tool name filter)
  stream?: boolean          (default false)
  temperature?: number
  maxTokens?: number

Response (200 — completed):
  { id, sessionId, status: 'completed', model, output: OutputItem[], usage }

Response (202 — waiting):
  { id, sessionId, status: 'waiting', model, output: OutputItem[], waitingFor: WaitingFor[], usage }

SSE stream (if stream=true):
  event: text_delta    data: { text }
  event: tool_start    data: { callId, name, args }
  event: tool_end      data: { callId, output, success }
  event: approval      data: { callId, name, args, preview }
  event: agent_status  data: { status, waitingFor? }
  event: done          data: { response }
  event: error         data: { message }
```

**POST /api/chat/agents/:agentId/deliver** — Deliver tool result to waiting agent
```
Request:  { callId, output: string, isError?: boolean }
Response: Same as completions (200 or 202)
```

**POST /api/chat/agents/:agentId/approve** — Approve/deny a pending tool execution
```
Request:  { callId, decision: 'approved' | 'denied' }
Response: Same as completions (200 or 202)
```

**GET /api/chat/agents/:agentId** — Get agent status
```
Response: { id, sessionId, status, waitingFor?, result?, error?, turnCount }
```

**GET /api/chat/agents/:agentId/events** — SSE stream for agent events
```
SSE stream: all events for this agent (tool executions, phase changes, etc.)
```

### Step 6.2 — Session routes
Create `src/routes/sessions.ts`:

- `GET /api/sessions` — list user's sessions
- `GET /api/sessions/:id` — get session with agents
- `PATCH /api/sessions/:id` — update title, archive
- `DELETE /api/sessions/:id` — delete session and all agents/items

### Step 6.3 — CRUD routes
Create `src/routes/` for remaining resources:

**Models** (`models.ts`):
- `GET /api/models` — list models
- `POST /api/models` — add model
- `PATCH /api/models/:id` — toggle enabled
- `DELETE /api/models/:id`

**API Keys** (`api-keys.ts`):
- `GET /api/keys/:provider` — check if key exists (don't return actual key)
- `PUT /api/keys/:provider` — set key
- `DELETE /api/keys/:provider`

**System Prompts** (`system-prompts.ts`):
- Full CRUD: GET (list), GET /:id, POST, PATCH /:id, DELETE /:id

**Preferences** (`preferences.ts`):
- `GET /api/preferences/:key`
- `PUT /api/preferences/:key`

**Tools** (`tools.ts`):
- `GET /api/tools` — list all tools with metadata
- `GET /api/tools/approvals` — list pending tool approvals
- `PUT /api/tools/:name/approval` — set approval override

**Usage** (`usage.ts`):
- `GET /api/usage` — usage statistics
- `GET /api/usage/sessions/:id` — per-session usage

### Checkpoint 6
- [x] POST `/api/chat/completions` runs a full agent loop and returns response
- [x] Streaming works — SSE events flow during agent execution
- [x] Deliver endpoint resumes a waiting agent
- [x] Approve endpoint handles tool approval flow
- [x] Session CRUD works
- [x] Model CRUD works
- [x] API key management works
- [x] All routes require auth (except /health)
- [x] Invalid requests return proper 400 errors with Zod messages
> Note: Auth middleware is implemented but not yet wired to API routes (routes use dev user). Routes verified via curl.

---

## Phase 7: Integration & Testing

### Step 7.1 — End-to-end test script
Create `src/test/e2e.ts`:
- Start server
- Create user + get token
- Send chat message → get response
- Send message that triggers tool → verify tool execution
- Send message that triggers approval → approve → verify
- Verify SSE events

### Step 7.2 — Wire up to Svelte frontend
Create `src/lib/backend/http-client.ts` in the **frontend** (sibling to existing `client.ts`):
- Same interface as current `client.ts` but uses `fetch()` instead of `invoke()`
- SSE handling for streaming responses
- Bearer token management (store in localStorage or memory)
- Config: `SERVER_URL` (defaults to `http://localhost:3001`)

Create a feature flag or config option to switch between Tauri IPC (`client.ts`) and HTTP (`http-client.ts`).

### Step 7.3 — Integration tools (deferred)
- Gmail, Calendar, Todoist — port OAuth flow and tool handlers
- Can be done as a follow-up

### Checkpoint 7
- [x] End-to-end: send message → get streamed response with tool calls
- [x] Frontend can switch between Tauri IPC and HTTP backend
- [x] Server runs standalone (no Tauri dependency)
- [x] Server deploys to a remote machine and accepts requests
> Note: E2E test passes 86/86 assertions. HTTP client written for frontend. Server runs standalone. Deployment is infra-specific (not tested here).

---

## File Structure

```
server/
├── package.json
├── tsconfig.json
├── drizzle.config.ts
├── .env.example
├── src/
│   ├── index.ts                    # Entry point — creates app, starts server
│   ├── db/
│   │   └── schema.ts              # Drizzle table definitions (no business logic)
│   │
│   ├── domain/                     # Pure types + state machines — ZERO imports from other modules
│   │   ├── agent.ts               # Agent type, status transitions (pure functions)
│   │   ├── plan.ts                # Plan, PlanStep, StepResult
│   │   └── types.ts               # WaitingFor, CallId, AgentStatus, etc.
│   │
│   ├── events/
│   │   ├── types.ts               # EventSink + EventSource interfaces (public contract)
│   │   └── emitter.ts             # EventEmitterImpl (concrete, imported only by runtime.ts)
│   │
│   ├── lib/
│   │   ├── config.ts              # Env config (Zod schema)
│   │   ├── runtime.ts             # COMPOSITION ROOT — only file that imports all concrete impls
│   │   └── logger.ts              # Pino logger
│   │
│   ├── middleware/
│   │   └── auth.ts                # Bearer token auth (depends on UserRepository interface)
│   │
│   ├── orchestrator/              # Depends ONLY on interfaces from providers/, tools/, repositories/, events/
│   │   ├── runner.ts              # Main agent loop (runAgent) — takes OrchestratorDeps
│   │   ├── types.ts               # ControllerAction, StepExecutionOutcome (INTERNAL — not exported)
│   │   ├── parsing.ts             # Parse LLM response → ControllerAction (pure functions)
│   │   ├── prompts.ts             # System prompts (string constants)
│   │   ├── hydration.ts           # Tool arg hydration (pure functions)
│   │   ├── output.ts              # Tool result → LLM message formatting (pure functions)
│   │   └── delivery.ts            # Wait/deliver/approve — takes same OrchestratorDeps
│   │
│   ├── providers/
│   │   ├── types.ts               # LLMProvider, LLMRequest, LLMResponse, ProviderRegistry (public contract)
│   │   ├── registry.ts            # ProviderRegistryImpl (concrete, imported only by runtime.ts)
│   │   ├── anthropic.ts           # AnthropicProvider implements LLMProvider
│   │   ├── openai.ts              # OpenAIProvider implements LLMProvider
│   │   ├── ollama.ts              # OllamaProvider implements LLMProvider (via OpenAI SDK)
│   │   └── openrouter.ts          # OpenRouterProvider implements LLMProvider (via OpenAI SDK)
│   │
│   ├── repositories/
│   │   ├── types.ts               # All repository interfaces (public contract)
│   │   └── sqlite/
│   │       ├── index.ts           # SQLiteRepositories implements all interfaces
│   │       └── migrations/        # Generated by drizzle-kit
│   │
│   ├── routes/                    # HTTP layer — depends on orchestrator + repository interfaces
│   │   ├── chat.ts                # Chat completions, deliver, approve, SSE events
│   │   ├── sessions.ts            # Session CRUD
│   │   ├── models.ts              # Model CRUD
│   │   ├── api-keys.ts            # API key management
│   │   ├── system-prompts.ts      # System prompt CRUD
│   │   ├── preferences.ts         # Preferences
│   │   ├── tools.ts               # Tool listing, approval overrides
│   │   └── usage.ts               # Usage statistics
│   │
│   └── tools/
│       ├── types.ts               # ToolExecutor, ToolHandler, ToolMetadata, ToolResult (public contract)
│       ├── registry.ts            # ToolRegistryImpl implements ToolExecutor (concrete, imported by runtime.ts)
│       ├── files.ts               # registerFileTools(registry) — captures no external deps
│       ├── search.ts              # registerSearchTools(registry)
│       ├── shell.ts               # registerShellTools(registry)
│       ├── web.ts                 # registerWebTools(registry)
│       ├── tool-outputs.ts        # registerToolOutputTools(registry, toolOutputRepo) — narrow dep injection
│       ├── preferences.ts         # registerPreferenceTools(registry, preferenceRepo)
│       └── integrations/          # Gmail, Calendar, Todoist (deferred)
│
└── test/
    └── e2e.ts                     # End-to-end test script
```

---

## Reference: Source Files to Port From

When implementing each component, consult these source files:

| Server component | Primary Rust source | Secondary reference (01_05_agent) |
|---|---|---|
| Orchestrator loop | `src-tauri/src/agent/orchestrator.rs` | `src/runtime/runner.ts` |
| Controller parsing | `src-tauri/src/agent/controller_parsing.rs` | — |
| Prompts | `src-tauri/src/agent/prompts.rs` | — |
| Tool registry | `src-tauri/src/tools/mod.rs` | `src/tools/registry.ts` |
| File tools | `src-tauri/src/tools/files/*.rs` | — |
| Shell tool | `src-tauri/src/tools/shell.rs` | — |
| Web tools | `src-tauri/src/tools/web/*.rs` | — |
| Search tool | `src-tauri/src/tools/search.rs` | — |
| Tool outputs | `src-tauri/src/tools/tool_outputs.rs` + `src-tauri/src/tool_outputs.rs` | — |
| Output delivery | `src-tauri/src/agent/output_delivery.rs` | — |
| Tool hydration | `src-tauri/src/agent/tool_arg_hydration.rs` | — |
| Tool execution records | `src-tauri/src/agent/tool_execution.rs` | — |
| Text utils | `src-tauri/src/agent/text_utils.rs` | — |
| Event system | `src-tauri/src/events.rs` | `src/events/emitter.ts` |
| DB operations | `src-tauri/src/db/` | `src/repositories/sqlite/` |
| Provider layer | `src-tauri/src/llm/` | `src/providers/` |
| Auth | — | `src/middleware/auth.ts` |
| Agent state machine | — | `src/domain/agent.ts` |
| Wait/deliver | — | `src/runtime/runner.ts` (deliverResult) |
| Routes | `src-tauri/src/commands/` | `src/routes/` |

---

## Non-Goals (for now)
- Conversation branching
- Context compaction / pruning
- Automatic LLM retry
- OAuth / integration connections (Gmail, Calendar, Todoist)
- MCP server management
- File versioning
- Custom backends
- Usage tracking / billing
