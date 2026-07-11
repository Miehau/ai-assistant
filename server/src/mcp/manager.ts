import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js'
import { UnauthorizedError } from '@modelcontextprotocol/sdk/client/auth.js'
import { v4 as uuid } from 'uuid'
import { z } from 'zod'
import type { ToolHandler, ToolMetadata, ToolResult } from '../tools/types.js'
import { ToolRegistryImpl } from '../tools/registry.js'
import { decrypt, deriveKey, encrypt } from '../lib/crypto.js'
import { logger } from '../lib/logger.js'
import type { McpRepository, SessionRepository, StoredMcpOAuthSession, StoredMcpServer, StoredMcpTool } from '../repositories/types.js'
import type {
  CreateMcpServerInput,
  McpServerRecord,
  McpServerStatus,
  McpOAuthSessionRecord,
  McpToolRecord,
  McpToolSnapshot,
  McpTransport,
  UpdateMcpServerInput,
} from './types.js'
import { PersistentMcpOAuthProvider, type PendingAuthorization } from './oauth-provider.js'
import { buildOAuthCallbackUrl } from './oauth-config.js'

type McpClient = Client

const createServerSchema = z.object({
  name: z.string().min(1).max(80),
  transport: z.enum(['stdio', 'streamable_http']),
  command: z.string().optional().nullable(),
  args: z.array(z.string()).optional(),
  env: z.record(z.string()).optional(),
  cwd: z.string().optional().nullable(),
  url: z.string().url().optional().nullable(),
  bearerToken: z.string().optional().nullable(),
  authMode: z.enum(['auto', 'none', 'bearer', 'oauth']).optional(),
  enabled: z.boolean().optional(),
})

const updateServerSchema = createServerSchema.partial()

export class McpManager {
  private clients = new Map<string, McpClient>()
  private registeredTools = new Set<string>()
  private encKey: string | null

  constructor(
    private readonly repository: McpRepository,
    private readonly sessions: SessionRepository,
    private readonly tools: ToolRegistryImpl,
    encryptionKey?: string,
    private readonly publicBaseUrl?: string,
    private readonly nodeEnv: 'development' | 'production' | 'test' = 'development',
  ) {
    this.encKey = encryptionKey ? deriveKey(encryptionKey) : null
  }

  async initialize(userIds: string[]): Promise<void> {
    for (const userId of userIds) {
      await this.registerKnownTools(userId)
      const enabled = await this.repository.listEnabledServers(userId)
      for (const server of enabled) await this.connect(userId, server.id)
    }
  }

  async shutdown(): Promise<void> {
    await Promise.all([...this.clients.values()].map(closeClient))
    this.clients.clear()
  }

  async listServers(userId: string): Promise<McpServerRecord[]> {
    const servers = await this.repository.listServers(userId)
    return Promise.all(servers.map((server) => this.buildServerRecord(userId, server)))
  }

  async getServer(userId: string, id: string): Promise<McpServerRecord | null> {
    const server = await this.repository.getServer(userId, id)
    return server ? this.buildServerRecord(userId, server) : null
  }

  async createServer(userId: string, input: unknown): Promise<McpServerRecord> {
    const parsed = createServerSchema.parse(input) satisfies CreateMcpServerInput
    this.validateServerConfig(parsed)
    const now = Date.now()
    const id = uuid()
    await this.repository.createServer({
      id,
      userId,
      authMode: resolveAuthMode(parsed.transport, parsed.bearerToken, parsed.authMode),
      name: parsed.name,
      transport: parsed.transport,
      command: parsed.command ?? null,
      args: JSON.stringify(parsed.args ?? []),
      env: JSON.stringify(parsed.env ?? {}),
      cwd: parsed.cwd ?? null,
      url: parsed.url ?? null,
      bearerToken: parsed.bearerToken ? this.storeSecret(parsed.bearerToken) : null,
      enabled: Boolean(parsed.enabled),
      status: parsed.enabled ? 'error' : 'disabled',
      error: null,
      createdAt: now,
      updatedAt: now,
    })

    if (parsed.enabled) {
      await this.connect(userId, id)
    }
    const server = await this.getServer(userId, id)
    if (!server) throw new Error(`MCP server not found after create: ${id}`)
    return server
  }

  async updateServer(userId: string, id: string, input: unknown): Promise<McpServerRecord> {
    const existing = await this.repository.getServer(userId, id)
    if (!existing) throw new Error(`MCP server not found: ${id}`)

    const parsed = updateServerSchema.parse(input) satisfies UpdateMcpServerInput
    const next = {
      name: parsed.name ?? existing.name,
      transport: (parsed.transport ?? existing.transport) as McpTransport,
      command: parsed.command !== undefined ? parsed.command : existing.command,
      args: parsed.args !== undefined ? parsed.args : parseJson<string[]>(existing.args, []),
      env: parsed.env !== undefined ? parsed.env : parseJson<Record<string, string>>(existing.env, {}),
      cwd: parsed.cwd !== undefined ? parsed.cwd : existing.cwd,
      url: parsed.url !== undefined ? parsed.url : existing.url,
      bearerToken: parsed.bearerToken !== undefined ? parsed.bearerToken : this.readSecret(existing.bearerToken),
      authMode: parsed.authMode ?? existing.authMode,
      enabled: parsed.enabled ?? Boolean(existing.enabled),
    }
    this.validateServerConfig(next)

    await this.disconnect(userId, id, next.enabled ? 'error' : 'disabled')

    await this.repository.updateServer(userId, id, {
      name: next.name,
      transport: next.transport,
      command: next.command ?? null,
      args: JSON.stringify(next.args ?? []),
      env: JSON.stringify(next.env ?? {}),
      cwd: next.cwd ?? null,
      url: next.url ?? null,
      bearerToken: next.bearerToken ? this.storeSecret(next.bearerToken) : null,
      authMode: resolveAuthMode(next.transport, next.bearerToken, next.authMode),
      enabled: next.enabled,
      status: next.enabled ? 'error' : 'disabled',
      error: null,
      updatedAt: Date.now(),
    })

    if (next.enabled) {
      await this.connect(userId, id)
    }

    const server = await this.getServer(userId, id)
    if (!server) throw new Error(`MCP server not found after update: ${id}`)
    return server
  }

  async deleteServer(userId: string, id: string): Promise<boolean> {
    const existing = await this.getServer(userId, id)
    if (!existing) return false
    await this.disconnect(userId, id, 'disabled')
    for (const tool of existing.tools) {
      this.tools.unregister(tool.registeredName)
      this.registeredTools.delete(tool.registeredName)
    }
    return this.repository.deleteServerAndTools(userId, id)
  }

  async setToolEnabled(userId: string, serverId: string, remoteName: string, enabled: boolean): Promise<McpToolRecord> {
    const tool = await this.repository.getToolByServerAndRemoteName(userId, serverId, remoteName)
    if (!tool) throw new Error(`MCP tool not found: ${remoteName}`)

    await this.repository.updateTool(userId, tool.id, { enabledForNewSessions: enabled, updatedAt: Date.now() })
    const updated = await this.repository.getTool(userId, tool.id)
    if (!updated) throw new Error(`MCP tool not found after update: ${remoteName}`)
    return this.toToolRecord(updated)
  }

  async getNewSessionToolSnapshot(userId: string, serverIds: string[]): Promise<McpToolSnapshot[]> {
    if (serverIds.length === 0) return []
    const servers = (await Promise.all(serverIds.map((id) => this.repository.getServer(userId, id))))
      .filter((server): server is StoredMcpServer => Boolean(server?.enabled))
    const enabledServerIds = new Set(servers.map((server) => server.id))
    if (enabledServerIds.size === 0) return []

    return (await this.repository.listTools(userId))
      .filter((tool) => enabledServerIds.has(tool.serverId) && tool.enabledForNewSessions)
      .map((tool) => ({
        name: tool.registeredName,
        description: this.toToolRecord(tool).description,
        parameters: this.toToolRecord(tool).inputSchema,
        requires_approval: true,
        source_id: tool.serverId,
      }))
  }

  async connectServer(userId: string, id: string): Promise<McpServerRecord> {
    await this.repository.updateServer(userId, id, { enabled: true, updatedAt: Date.now() })
    await this.connect(userId, id)
    const server = await this.getServer(userId, id)
    if (!server) throw new Error(`MCP server not found: ${id}`)
    return server
  }

  async startOAuth(userId: string, id: string): Promise<PendingAuthorization | null> {
    const server = await this.repository.getServer(userId, id)
    if (!server) throw new Error(`MCP server not found: ${id}`)
    if (server.transport !== 'streamable_http' || !['auto', 'oauth'].includes(server.authMode)) {
      throw new Error('OAuth is not available for this MCP server')
    }
    if (!this.encKey) throw new Error('ENCRYPTION_KEY is required for MCP OAuth')
    this.oauthRedirectUrl()
    await this.repository.updateServer(userId, id, { enabled: true, updatedAt: Date.now() })
    await this.disconnect(userId, id, 'error')
    return this.connect(userId, id, true)
  }

  async finishOAuth(userId: string, serverId: string, sessionId: string, authorizationCode: string): Promise<McpServerRecord> {
    const server = await this.repository.getServer(userId, serverId)
    if (!server?.url) throw new Error('MCP server not found')
    const session = await this.repository.getOAuthSession(userId, serverId, sessionId)
    if (!session || session.status !== 'pending' || !session.consumedAt) throw new Error('OAuth authorization session is no longer valid')
    const provider = this.createOAuthProvider(server, sessionId)
    const transport = this.createTransport(server, provider)
    try {
      await transport.finishAuth(authorizationCode)
      await this.repository.updateOAuthSession(userId, serverId, sessionId, {
        status: 'authorized', error: null, updatedAt: Date.now(),
      })
      await this.connect(userId, serverId)
    } catch (error) {
      await this.repository.updateOAuthSession(userId, serverId, sessionId, {
        status: 'error', error: safeMcpError(error), updatedAt: Date.now(),
      })
      await this.setServerStatus(userId, serverId, 'error', safeMcpError(error))
      throw new Error(safeMcpError(error))
    }
    const record = await this.getServer(userId, serverId)
    if (!record) throw new Error('MCP server not found')
    return record
  }

  async cancelOAuth(userId: string, serverId: string): Promise<McpServerRecord> {
    const session = await this.repository.getOAuthSession(userId, serverId)
    if (session?.status === 'pending') {
      await this.repository.updateOAuthSession(userId, serverId, session.id, {
        status: 'cancelled', error: 'Authorization was cancelled.', updatedAt: Date.now(),
      })
    }
    await this.disconnect(userId, serverId, 'error')
    const record = await this.getServer(userId, serverId)
    if (!record) throw new Error(`MCP server not found: ${serverId}`)
    return record
  }

  async disconnectServer(userId: string, serverId: string, clearOAuth = false): Promise<McpServerRecord> {
    await this.disconnect(userId, serverId, 'disabled')
    await this.repository.updateServer(userId, serverId, { enabled: false, updatedAt: Date.now() })
    if (clearOAuth) await this.repository.deleteOAuthCredentials(userId, serverId)
    const record = await this.getServer(userId, serverId)
    if (!record) throw new Error(`MCP server not found: ${serverId}`)
    return record
  }

  isMcpToolName(name: string): boolean {
    return name.startsWith('mcp.')
  }

  private async connect(userId: string, id: string, forceOAuth = false): Promise<PendingAuthorization | null> {
    const server = await this.repository.getServer(userId, id)
    if (!server) throw new Error(`MCP server not found: ${id}`)

    await this.setServerStatus(userId, id, 'connecting', null)
    let provider: PersistentMcpOAuthProvider | undefined
    try {
      const client = new Client({ name: 'ai-frontend-server', version: '0.1.0' }, { capabilities: {} })
      const canAttemptOAuth = Boolean(this.publicBaseUrl && this.encKey)
      if (server.transport === 'streamable_http' && (forceOAuth || server.authMode === 'oauth' || (server.authMode === 'auto' && canAttemptOAuth))) {
        provider = this.createOAuthProvider(server)
      }
      const transport = this.createTransport(server, provider)
      await client.connect(transport)
      this.clients.set(id, client)

      const response = await client.listTools()
      const remoteTools = response.tools ?? []
      await this.upsertDiscoveredTools(userId, server.id, server.name, remoteTools)
      await this.registerKnownTools(userId, server.id)
      await this.setServerStatus(userId, id, 'connected', null)
      return null
    } catch (err) {
      await this.disconnect(userId, id, 'error')
      const pending = provider?.takePendingAuthorization() ?? null
      if (pending) await provider?.invalidateCredentials('tokens')
      const message = pending || err instanceof UnauthorizedError ? null : safeMcpError(err)
      await this.setServerStatus(userId, id, 'error', message)
      logger.warn({ serverId: id, errorType: errorType(err) }, 'MCP server connection failed')
      return pending
    }
  }

  private async disconnect(userId: string, id: string, status: McpServerStatus): Promise<void> {
    const client = this.clients.get(id)
    if (client) {
      await closeClient(client)
    }
    this.clients.delete(id)
    await this.setServerStatus(userId, id, status, status === 'disabled' ? null : undefined)
  }

  private createTransport(server: StoredMcpServer, authProvider?: PersistentMcpOAuthProvider): any {
    if (server.transport === 'stdio') {
      return new StdioClientTransport({
        command: server.command ?? '',
        args: parseJson<string[]>(server.args, []),
        env: { ...stringEnv(process.env), ...parseJson<Record<string, string>>(server.env, {}) },
        cwd: server.cwd ?? undefined,
      })
    }

    const headers: Record<string, string> = {}
    const token = server.authMode === 'bearer' ? this.readSecret(server.bearerToken) : null
    if (token) headers.Authorization = `Bearer ${token}`
    return new StreamableHTTPClientTransport(new URL(server.url ?? ''), {
      requestInit: { headers },
      ...(authProvider ? { authProvider } : {}),
    })
  }

  private createOAuthProvider(server: StoredMcpServer, sessionId?: string): PersistentMcpOAuthProvider {
    if (!server.url) throw new Error('url is required for MCP OAuth')
    return new PersistentMcpOAuthProvider(
      this.repository, server.userId, server.id, server.url, this.oauthRedirectUrl(), sessionId,
    )
  }

  private oauthRedirectUrl(): string {
    return buildOAuthCallbackUrl(this.publicBaseUrl, this.nodeEnv)
  }

  private async registerKnownTools(userId: string, serverId?: string): Promise<void> {
    const rows = await this.repository.listTools(userId, serverId)

    for (const row of rows) {
      const server = await this.repository.getServer(userId, row.serverId)
      if (!server) continue
      const registeredName = `mcp.${sanitizeName(server.name)}_${server.id.slice(0, 8)}.${sanitizeName(row.remoteName)}`
      if (row.registeredName !== registeredName) {
        await this.repository.updateTool(userId, row.id, { registeredName, updatedAt: Date.now() })
      }
      const tool = this.toToolRecord({ ...row, registeredName })
      if (this.registeredTools.has(tool.registeredName) || this.tools.getMetadata(tool.registeredName)) continue
      const handler: ToolHandler = {
        metadata: {
          name: tool.registeredName,
          description: tool.description,
          parameters: tool.inputSchema,
          requires_approval: true,
        },
        handle: (args, ctx) => this.callTool(userId, ctx.session_id, tool.serverId, tool.remoteName, args, ctx.signal),
        preview: () => ({ summary: `Call MCP tool ${tool.remoteName}` }),
      }
      this.tools.register(handler)
      this.registeredTools.add(tool.registeredName)
    }
  }

  private async callTool(
    userId: string,
    sessionId: string,
    serverId: string,
    remoteName: string,
    args: Record<string, unknown>,
    _signal: AbortSignal,
  ): Promise<ToolResult> {
    const session = await this.sessions.getById(sessionId)
    if (!session || session.userId !== userId) return { ok: false, error: 'MCP tool is unavailable for this session' }
    const client = this.clients.get(serverId)
    if (!client) {
      const server = await this.getServer(userId, serverId)
      return { ok: false, error: `MCP server is not connected: ${server?.name ?? serverId}` }
    }

    try {
      const result = await client.callTool({ name: remoteName, arguments: args })
      return mapCallResult(result)
    } catch (err) {
      const server = await this.repository.getServer(userId, serverId)
      return {
        ok: false,
        error: server && ['auto', 'oauth'].includes(server.authMode)
          ? safeMcpError(err)
          : err instanceof Error ? err.message : String(err),
      }
    }
  }

  private async upsertDiscoveredTools(
    userId: string,
    serverId: string,
    serverName: string,
    tools: Array<{ name: string; description?: string; inputSchema?: Record<string, unknown> }>,
  ): Promise<void> {
    const now = Date.now()
    const existing = await this.repository.listTools(userId, serverId)
    const existingByRemoteName = new Map(existing.map((tool) => [tool.remoteName, tool]))

    for (const tool of tools) {
      const current = existingByRemoteName.get(tool.name)
      const registeredName = `mcp.${sanitizeName(serverName)}_${serverId.slice(0, 8)}.${sanitizeName(tool.name)}`
      if (current) {
        await this.repository.updateTool(userId, current.id, {
          registeredName,
          description: tool.description ?? '',
          inputSchema: JSON.stringify(normalizeSchema(tool.inputSchema)),
          updatedAt: now,
        })
      } else {
        await this.repository.createTool(userId, {
          id: uuid(),
          serverId,
          remoteName: tool.name,
          registeredName,
          description: tool.description ?? '',
          inputSchema: JSON.stringify(normalizeSchema(tool.inputSchema)),
          enabledForNewSessions: true,
          createdAt: now,
          updatedAt: now,
        })
      }
    }
  }

  private async getToolsForServer(userId: string, serverId: string): Promise<McpToolRecord[]> {
    return (await this.repository.listTools(userId, serverId)).map((tool) => this.toToolRecord(tool))
  }

  private async buildServerRecord(userId: string, server: StoredMcpServer): Promise<McpServerRecord> {
    const [tools, credentials, session] = await Promise.all([
      this.getToolsForServer(userId, server.id),
      this.repository.getOAuthCredentials(userId, server.id),
      this.repository.getOAuthSession(userId, server.id),
    ])
    return this.toServerRecord(server, tools, Boolean(credentials?.tokens), session ? toSafeSession(session) : null)
  }

  private toServerRecord(
    row: StoredMcpServer,
    tools: McpToolRecord[],
    oauthCredentialsConfigured: boolean,
    oauthSession: McpOAuthSessionRecord | null,
  ): McpServerRecord {
    return {
      id: row.id,
      name: row.name,
      transport: row.transport as McpTransport,
      command: row.command ?? null,
      args: parseJson(row.args, []),
      env: parseJson(row.env, {}),
      cwd: row.cwd ?? null,
      url: row.url ?? null,
      bearerTokenConfigured: Boolean(row.bearerToken),
      authMode: row.authMode,
      authStatus: authStatus(row, oauthCredentialsConfigured, oauthSession),
      connectionStatus: row.status as McpServerRecord['connectionStatus'],
      oauthCredentialsConfigured,
      oauthSession,
      enabled: row.enabled,
      status: row.status as McpServerStatus,
      error: row.error ?? null,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
      tools,
    }
  }

  private toToolRecord(row: StoredMcpTool): McpToolRecord {
    return {
      id: row.id,
      serverId: row.serverId,
      remoteName: row.remoteName,
      registeredName: row.registeredName,
      description: row.description ?? '',
      inputSchema: normalizeSchema(parseJson(row.inputSchema, { type: 'object', properties: {} })),
      enabledForNewSessions: row.enabledForNewSessions,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    }
  }

  private async setServerStatus(userId: string, id: string, status: McpServerStatus, error?: string | null): Promise<void> {
    await this.repository.updateServer(userId, id, {
      status,
      ...(error !== undefined ? { error } : {}),
      updatedAt: Date.now(),
    })
  }

  private validateServerConfig(input: {
    transport: McpTransport
    command?: string | null
    url?: string | null
  }): void {
    if (input.transport === 'stdio' && !input.command) {
      throw new Error('command is required for stdio MCP servers')
    }
    if (input.transport === 'streamable_http' && !input.url) {
      throw new Error('url is required for Streamable HTTP MCP servers')
    }
  }

  private storeSecret(value: string): string {
    return this.encKey ? encrypt(value, this.encKey) : value
  }

  private readSecret(value?: string | null): string | null {
    if (!value) return null
    return this.encKey ? decrypt(value, this.encKey) : value
  }
}

function resolveAuthMode(transport: McpTransport, bearerToken: string | null | undefined, requested?: CreateMcpServerInput['authMode']): NonNullable<CreateMcpServerInput['authMode']> {
  if (transport === 'stdio') return 'none'
  if (requested && requested !== 'auto') return requested
  return bearerToken ? 'bearer' : 'auto'
}

function toSafeSession(session: StoredMcpOAuthSession): McpOAuthSessionRecord {
  return {
    id: session.id,
    serverId: session.serverId,
    status: session.status,
    expiresAt: session.expiresAt,
    error: session.error,
  }
}

function authStatus(
  server: StoredMcpServer,
  credentialsConfigured: boolean,
  session: McpOAuthSessionRecord | null,
): McpServerRecord['authStatus'] {
  if (server.authMode === 'none' || server.authMode === 'bearer') return 'not_required'
  if (session?.status === 'pending') return 'pending'
  if (credentialsConfigured) return 'authorized'
  if (session?.status === 'error') return 'error'
  if (server.authMode === 'auto' && server.status === 'connected') return 'not_required'
  return 'required'
}

function safeMcpError(error: unknown): string {
  if (error instanceof UnauthorizedError) return 'Sign in to continue.'
  const message = error instanceof Error ? error.message : ''
  if (message.includes('resource') && (message.includes('match') || message.includes('URL'))) {
    return 'OAuth resource does not match the configured MCP server.'
  }
  if (message.includes('ENCRYPTION_KEY') || message.includes('PUBLIC_BASE_URL')) return message
  return 'Could not connect to the MCP server.'
}

function errorType(error: unknown): string {
  return error instanceof Error ? error.constructor.name : typeof error
}

async function closeClient(client: McpClient): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    await Promise.race([
      client.close(),
      new Promise<void>((resolve) => { timer = setTimeout(resolve, 2_000) }),
    ])
  } catch {
    // Shutdown is best effort; connection state is cleared by the caller.
  } finally {
    if (timer) clearTimeout(timer)
  }
}

function parseJson<T>(value: string | null | undefined, fallback: T): T {
  if (!value) return fallback
  try {
    return JSON.parse(value) as T
  } catch {
    return fallback
  }
}

function sanitizeName(name: string): string {
  return name.replace(/[^a-zA-Z0-9_-]/g, '_')
}

function normalizeSchema(schemaValue: unknown): Record<string, unknown> {
  if (schemaValue && typeof schemaValue === 'object' && !Array.isArray(schemaValue)) {
    return schemaValue as Record<string, unknown>
  }
  return { type: 'object', properties: {} }
}

function stringEnv(env: NodeJS.ProcessEnv): Record<string, string> {
  return Object.fromEntries(
    Object.entries(env).filter((entry): entry is [string, string] => typeof entry[1] === 'string'),
  )
}

function mapCallResult(result: Awaited<ReturnType<Client['callTool']>>): ToolResult {
  if (result.isError) {
    return { ok: false, error: formatMcpContent(result.content) }
  }

  const contentBlocks: ToolResult['content_blocks'] = []
  for (const block of (result.content ?? []) as Array<Record<string, any>>) {
    if (block.type === 'text') {
      contentBlocks.push({ type: 'text', text: block.text })
    } else if (block.type === 'image') {
      contentBlocks.push({ type: 'image', data: block.data, media_type: block.mimeType })
    }
  }

  return {
    ok: true,
    output: formatMcpContent(result.content) || result,
    ...(contentBlocks.length > 0 && { content_blocks: contentBlocks }),
  }
}

function formatMcpContent(content: Awaited<ReturnType<Client['callTool']>>['content']): string {
  return ((content ?? []) as Array<Record<string, any>>)
    .map((block) => {
      if (block.type === 'text') return block.text
      if (block.type === 'image') return `[image:${block.mimeType}]`
      if (block.type === 'resource') return JSON.stringify(block.resource)
      return JSON.stringify(block)
    })
    .join('\n')
}
