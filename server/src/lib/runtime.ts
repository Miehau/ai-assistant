import fs from 'fs/promises'
import path from 'path'
import { fileURLToPath } from 'node:url'

// server/src/lib → server/
const SERVER_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../')

import { createDatabase, SQLiteRepositories, seedDevUser } from '../repositories/sqlite/index.js'
import type { DrizzleInstance } from '../repositories/sqlite/index.js'
import { ProviderRegistryImpl } from '../providers/registry.js'
import { ToolRegistryImpl } from '../tools/registry.js'
import { AgentEventEmitter } from '../events/emitter.js'
import { registerFileTools } from '../tools/files.js'
import { registerShellTools } from '../tools/shell.js'
import { registerWebTools } from '../tools/web.js'
import { registerSearchTools } from '../tools/search.js'
import { registerNoteTools } from '../tools/notes.js'
import { registerToolOutputTools } from '../tools/tool-outputs.js'
import { registerPreferenceTools } from '../tools/preferences.js'
import { registerDelegateTools } from '../tools/delegate.js'
import { loadAgentDefinitions } from '../agents/loader.js'
import { AgentDefinitionRegistryImpl } from '../agents/registry.js'
import type { AgentDefinitionRegistry } from '../agents/registry.js'
import { registerTaskTools } from '../tools/tasks.js'
import { registerThinkTool } from '../tools/think.js'
import { logger } from './logger.js'
import type { AppConfig } from './config.js'
import type { ProviderRegistry } from '../providers/types.js'
import type { ToolExecutor } from '../tools/types.js'
import type { EventSink, EventSource } from '../events/types.js'
import type { InterceptHandler } from '../orchestrator/types.js'
import type { WorkflowRegistry } from '../workflows/types.js'
import type { WorkflowRunRepository } from '../repositories/types.js'
import { WorkflowRegistryImpl } from '../workflows/registry.js'
import { WorkflowExecutor } from '../workflows/executor.js'
import { registerWorkflowTools } from '../workflows/tool.js'
import { loadWorkflowDefinitions } from '../workflows/loader.js'
import { McpManager } from '../mcp/manager.js'
import type {
  UserRepository,
  SessionRepository,
  AgentRepository,
  ItemRepository,
  ToolOutputRepository,
  ModelRepository,
  ApiKeyRepository,
  SystemPromptRepository,
  PreferenceRepository,
} from '../repositories/types.js'

export interface RuntimeContext {
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
  mcps: McpManager
  events: EventSink & EventSource
  config: AppConfig
  db: DrizzleInstance
  agentDefinitions: AgentDefinitionRegistry
  /** Pluggable intercept handlers for orchestrator_intercept tools. */
  interceptHandlers: Map<string, InterceptHandler>
  /** Workflow subsystem — null if no workflows are registered. */
  workflows: {
    registry: WorkflowRegistry
    executor: WorkflowExecutor
    repository: WorkflowRunRepository
  } | null
  /** AbortController for graceful shutdown — aborts all in-flight agent runs. */
  shutdownController: AbortController
  /** Per-agent AbortControllers — keyed by agent ID, used to cancel individual runs. */
  agentAbortControllers: Map<string, AbortController>
}

export async function initRuntime(config: AppConfig): Promise<RuntimeContext> {
  // 1. Ensure data directory exists for SQLite file
  const dbDir = path.dirname(config.databaseUrl)
  await fs.mkdir(dbDir, { recursive: true })
  logger.info({ path: config.databaseUrl }, 'Opening database')

  // 2. Create database connection and repositories
  const db = createDatabase(config.databaseUrl)
  const repos = new SQLiteRepositories(db, config.encryptionKey)

  if (!config.encryptionKey) {
    logger.warn('ENCRYPTION_KEY not set — API keys stored without encryption. Set ENCRYPTION_KEY in .env for production.')
  }

  // 3. Seed dev user
  const devUser = await seedDevUser(db)
  logger.info({ userId: devUser.id, email: devUser.email }, 'Dev user ready')

  // 4. Create event emitter
  const events = new AgentEventEmitter()

  // 5. Load agent definitions from markdown files
  const agentsDir = path.isAbsolute(config.agentsDir)
    ? config.agentsDir
    : path.resolve(SERVER_ROOT, config.agentsDir)
  const agentDefs = await loadAgentDefinitions(agentsDir)
  const agentDefinitions = new AgentDefinitionRegistryImpl(agentsDir, agentDefs)
  logger.info({ count: agentDefs.length, dir: agentsDir }, 'Agent definitions loaded')

  // 6. Create tool registry and register all tools
  const tools = new ToolRegistryImpl()
  registerFileTools(tools)
  registerShellTools(tools)
  registerWebTools(tools)
  registerSearchTools(tools)
  registerToolOutputTools(tools, repos.toolOutputs)
  registerPreferenceTools(tools, repos.preferences)
  registerThinkTool(tools)

  // Task management tools — files stored in data/tasks/, outputs in data/workspace/
  const tasksDir = path.isAbsolute(config.tasksDir)
    ? config.tasksDir
    : path.resolve(SERVER_ROOT, config.tasksDir)
  const workspaceDir = path.isAbsolute(config.workspaceDir)
    ? config.workspaceDir
    : path.resolve(SERVER_ROOT, config.workspaceDir)
  registerTaskTools(tools, tasksDir, workspaceDir)
  registerNoteTools(tools, path.resolve(SERVER_ROOT, './data/research-notes'), [workspaceDir])

  // Intercept handlers — populated by register*Tools functions that provide orchestrator_intercept tools
  const interceptHandlers = new Map<string, InterceptHandler>()
  registerDelegateTools(tools, agentDefinitions, interceptHandlers)

  const mcps = new McpManager(db, tools, config.encryptionKey)
  await mcps.initialize()

  const toolCount = tools.listMetadata().length
  logger.info({ toolCount }, 'Tool registry initialized')

  // 6. Create provider registry
  //    Priority: .env keys → DB-stored keys (DB keys won't overwrite .env ones)
  const providers = new ProviderRegistryImpl()
  providers.setApiKeyRepository(repos.apiKeys)

  const envRegistered: string[] = []

  if (config.anthropicApiKey) {
    providers.registerFromKey('anthropic', config.anthropicApiKey)
    envRegistered.push('anthropic')
  }
  if (config.openaiApiKey) {
    providers.registerFromKey('openai', config.openaiApiKey)
    envRegistered.push('openai')
  }
  if (config.ollamaBaseUrl) {
    providers.registerFromKey('ollama', config.ollamaBaseUrl)
    envRegistered.push('ollama')
  }
  if (config.openrouterApiKey) {
    providers.registerFromKey('openrouter', config.openrouterApiKey)
    envRegistered.push('openrouter')
  }

  if (envRegistered.length > 0) {
    logger.info({ providers: envRegistered }, 'Providers registered from environment')
  }

  // Load any additional keys stored in the database (e.g. saved via the UI)
  const dbRegistered = await providers.loadKeysFromDatabase()

  if (envRegistered.length === 0 && dbRegistered.length === 0) {
    logger.warn('No LLM providers configured. Set keys via .env or PUT /api/keys/:provider')
  }

  // 7. Build workflow subsystem (two-phase: registry first, executor after providers)
  const workflowsDir = path.isAbsolute(config.workflowsDir)
    ? config.workflowsDir
    : path.resolve(SERVER_ROOT, config.workflowsDir)
  const workflowDefs = await loadWorkflowDefinitions(workflowsDir)
  const workflowRegistry = new WorkflowRegistryImpl()
  for (const def of workflowDefs) {
    workflowRegistry.register(def)
  }

  let workflows: RuntimeContext['workflows'] = null
  if (workflowDefs.length > 0) {
    const executor = new WorkflowExecutor({
      workflowRuns: repos.workflowRuns,
      events,
      tools,
      providers,
      agents: repos.agents,
      items: repos.items,
      toolOutputs: repos.toolOutputs,
      preferences: repos.preferences,
      agentDefinitions,
      interceptHandlers,
      defaultModel: config.defaultModel,
    })
    registerWorkflowTools(tools, workflowRegistry, executor, interceptHandlers)
    workflows = { registry: workflowRegistry, executor, repository: repos.workflowRuns }
    logger.info({ count: workflowDefs.length }, 'Workflow subsystem initialized')
  }

  // 8. Return RuntimeContext
  return {
    repositories: {
      users: repos.users,
      sessions: repos.sessions,
      agents: repos.agents,
      items: repos.items,
      toolOutputs: repos.toolOutputs,
      models: repos.models,
      apiKeys: repos.apiKeys,
      systemPrompts: repos.systemPrompts,
      preferences: repos.preferences,
    },
    providers,
    tools,
    mcps,
    events,
    config,
    db,
    agentDefinitions,
    interceptHandlers,
    workflows,
    shutdownController: new AbortController(),
    agentAbortControllers: new Map(),
  }
}

export async function shutdownRuntime(runtime: RuntimeContext): Promise<void> {
  logger.info('Shutting down runtime — aborting in-flight agents')

  // Abort in-flight workflow runs
  runtime.workflows?.executor.abortAll()
  await runtime.mcps.shutdown()

  // Signal all in-flight agent runs to abort
  for (const controller of runtime.agentAbortControllers.values()) {
    controller.abort()
  }
  runtime.agentAbortControllers.clear()
  runtime.shutdownController.abort()

  // Mark any running/waiting agents as failed so they don't appear stuck on restart
  try {
    const { agents } = await import('../db/schema.js')
    const { eq: eqOp, or: orOp } = await import('drizzle-orm')
    runtime.db.update(agents)
      .set({
        status: 'failed',
        error: 'Server shutdown',
        completedAt: Date.now(),
      })
      .where(
        orOp(
          eqOp(agents.status, 'running'),
          eqOp(agents.status, 'waiting'),
        )!,
      )
      .run()
  } catch {
    // Best-effort — don't block shutdown
  }

  logger.info('Runtime shutdown complete')
}
