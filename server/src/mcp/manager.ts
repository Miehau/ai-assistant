import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js'
import { v4 as uuid } from 'uuid'
import { z } from 'zod'
import type { ToolHandler, ToolMetadata, ToolResult } from '../tools/types.js'
import { ToolRegistryImpl } from '../tools/registry.js'
import { decrypt, deriveKey, encrypt } from '../lib/crypto.js'
import { logger } from '../lib/logger.js'
import type { McpRepository, StoredMcpServer, StoredMcpTool } from '../repositories/types.js'
import type {
  CreateMcpServerInput,
  McpServerRecord,
  McpServerStatus,
  McpToolRecord,
  McpToolSnapshot,
  McpTransport,
  UpdateMcpServerInput,
} from './types.js'

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
  enabled: z.boolean().optional(),
})

const updateServerSchema = createServerSchema.partial()

export class McpManager {
  private clients = new Map<string, McpClient>()
  private registeredTools = new Set<string>()
  private encKey: string | null

  constructor(
    private readonly repository: McpRepository,
    private readonly tools: ToolRegistryImpl,
    encryptionKey?: string,
  ) {
    this.encKey = encryptionKey ? deriveKey(encryptionKey) : null
  }

  async initialize(): Promise<void> {
    await this.registerKnownTools()
    const enabled = await this.repository.listEnabledServers()
    for (const server of enabled) {
      await this.connect(server.id)
    }
  }

  async shutdown(): Promise<void> {
    for (const client of this.clients.values()) {
      await client.close()
    }
    this.clients.clear()
  }

  async listServers(): Promise<McpServerRecord[]> {
    const servers = await this.repository.listServers()
    return Promise.all(servers.map(async (server) => this.toServerRecord(server, await this.getToolsForServer(server.id))))
  }

  async getServer(id: string): Promise<McpServerRecord | null> {
    const server = await this.repository.getServer(id)
    return server ? this.toServerRecord(server, await this.getToolsForServer(server.id)) : null
  }

  async createServer(input: unknown): Promise<McpServerRecord> {
    const parsed = createServerSchema.parse(input) satisfies CreateMcpServerInput
    this.validateServerConfig(parsed)
    const now = Date.now()
    const id = uuid()
    await this.repository.createServer({
      id,
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
      await this.connect(id)
    }
    const server = await this.getServer(id)
    if (!server) throw new Error(`MCP server not found after create: ${id}`)
    return server
  }

  async updateServer(id: string, input: unknown): Promise<McpServerRecord> {
    const existing = await this.repository.getServer(id)
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
      enabled: parsed.enabled ?? Boolean(existing.enabled),
    }
    this.validateServerConfig(next)

    await this.disconnect(id, next.enabled ? 'error' : 'disabled')

    await this.repository.updateServer(id, {
      name: next.name,
      transport: next.transport,
      command: next.command ?? null,
      args: JSON.stringify(next.args ?? []),
      env: JSON.stringify(next.env ?? {}),
      cwd: next.cwd ?? null,
      url: next.url ?? null,
      bearerToken: next.bearerToken ? this.storeSecret(next.bearerToken) : null,
      enabled: next.enabled,
      status: next.enabled ? 'error' : 'disabled',
      error: null,
      updatedAt: Date.now(),
    })

    if (next.enabled) {
      await this.connect(id)
    }

    const server = await this.getServer(id)
    if (!server) throw new Error(`MCP server not found after update: ${id}`)
    return server
  }

  async deleteServer(id: string): Promise<boolean> {
    const existing = await this.getServer(id)
    if (!existing) return false
    await this.disconnect(id, 'disabled')
    for (const tool of existing.tools) {
      this.tools.unregister(tool.registeredName)
      this.registeredTools.delete(tool.registeredName)
    }
    await this.repository.deleteServerAndTools(id)
    return true
  }

  async setToolEnabled(serverId: string, remoteName: string, enabled: boolean): Promise<McpToolRecord> {
    const tool = await this.repository.getToolByServerAndRemoteName(serverId, remoteName)
    if (!tool) throw new Error(`MCP tool not found: ${remoteName}`)

    await this.repository.updateTool(tool.id, { enabledForNewSessions: enabled, updatedAt: Date.now() })
    const updated = await this.repository.getTool(tool.id)
    if (!updated) throw new Error(`MCP tool not found after update: ${remoteName}`)
    return this.toToolRecord(updated)
  }

  async getNewSessionToolSnapshot(serverIds: string[]): Promise<McpToolSnapshot[]> {
    if (serverIds.length === 0) return []
    const servers = (await Promise.all(serverIds.map((id) => this.repository.getServer(id))))
      .filter((server): server is StoredMcpServer => Boolean(server?.enabled))
    const enabledServerIds = new Set(servers.map((server) => server.id))
    if (enabledServerIds.size === 0) return []

    return (await this.repository.listTools())
      .filter((tool) => enabledServerIds.has(tool.serverId) && tool.enabledForNewSessions)
      .map((tool) => ({
        name: tool.registeredName,
        description: this.toToolRecord(tool).description,
        parameters: this.toToolRecord(tool).inputSchema,
        requires_approval: true,
        source_id: tool.serverId,
      }))
  }

  isMcpToolName(name: string): boolean {
    return name.startsWith('mcp.')
  }

  private async connect(id: string): Promise<void> {
    const server = await this.repository.getServer(id)
    if (!server) throw new Error(`MCP server not found: ${id}`)

    try {
      const client = new Client({ name: 'ai-frontend-server', version: '0.1.0' }, { capabilities: {} })
      const transport = this.createTransport(server)
      await client.connect(transport)
      this.clients.set(id, client)

      const response = await client.listTools()
      const remoteTools = response.tools ?? []
      await this.upsertDiscoveredTools(server.id, server.name, remoteTools)
      await this.registerKnownTools(server.id)
      await this.setServerStatus(id, 'connected', null)
    } catch (err) {
      await this.disconnect(id, 'error')
      const message = err instanceof Error ? err.message : String(err)
      await this.setServerStatus(id, 'error', message)
      logger.warn({ err, serverId: id }, 'MCP server connection failed')
    }
  }

  private async disconnect(id: string, status: McpServerStatus): Promise<void> {
    const client = this.clients.get(id)
    if (client) {
      try {
        await client.close()
      } catch {
        // Ignore shutdown errors.
      }
    }
    this.clients.delete(id)
    await this.setServerStatus(id, status, status === 'disabled' ? null : undefined)
  }

  private createTransport(server: StoredMcpServer): any {
    if (server.transport === 'stdio') {
      return new StdioClientTransport({
        command: server.command ?? '',
        args: parseJson<string[]>(server.args, []),
        env: { ...stringEnv(process.env), ...parseJson<Record<string, string>>(server.env, {}) },
        cwd: server.cwd ?? undefined,
      })
    }

    const headers: Record<string, string> = {}
    const token = this.readSecret(server.bearerToken)
    if (token) headers.Authorization = `Bearer ${token}`
    return new StreamableHTTPClientTransport(new URL(server.url ?? ''), {
      requestInit: { headers },
    })
  }

  private async registerKnownTools(serverId?: string): Promise<void> {
    const rows = await this.repository.listTools(serverId)

    for (const row of rows) {
      const tool = this.toToolRecord(row)
      if (this.registeredTools.has(tool.registeredName) || this.tools.getMetadata(tool.registeredName)) continue
      const handler: ToolHandler = {
        metadata: {
          name: tool.registeredName,
          description: tool.description,
          parameters: tool.inputSchema,
          requires_approval: true,
        },
        handle: (args, ctx) => this.callTool(tool.serverId, tool.remoteName, args, ctx.signal),
        preview: () => ({ summary: `Call MCP tool ${tool.remoteName}` }),
      }
      this.tools.register(handler)
      this.registeredTools.add(tool.registeredName)
    }
  }

  private async callTool(
    serverId: string,
    remoteName: string,
    args: Record<string, unknown>,
    _signal: AbortSignal,
  ): Promise<ToolResult> {
    const client = this.clients.get(serverId)
    if (!client) {
      const server = await this.getServer(serverId)
      return { ok: false, error: `MCP server is not connected: ${server?.name ?? serverId}` }
    }

    try {
      const result = await client.callTool({ name: remoteName, arguments: args })
      return mapCallResult(result)
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) }
    }
  }

  private async upsertDiscoveredTools(
    serverId: string,
    serverName: string,
    tools: Array<{ name: string; description?: string; inputSchema?: Record<string, unknown> }>,
  ): Promise<void> {
    const now = Date.now()
    const existing = await this.repository.listTools(serverId)
    const existingByRemoteName = new Map(existing.map((tool) => [tool.remoteName, tool]))

    for (const tool of tools) {
      const current = existingByRemoteName.get(tool.name)
      const registeredName = `mcp.${sanitizeName(serverName)}.${sanitizeName(tool.name)}`
      if (current) {
        await this.repository.updateTool(current.id, {
          registeredName,
          description: tool.description ?? '',
          inputSchema: JSON.stringify(normalizeSchema(tool.inputSchema)),
          updatedAt: now,
        })
      } else {
        await this.repository.createTool({
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

  private async getToolsForServer(serverId: string): Promise<McpToolRecord[]> {
    return (await this.repository.listTools(serverId)).map((tool) => this.toToolRecord(tool))
  }

  private toServerRecord(
    row: StoredMcpServer,
    tools: McpToolRecord[],
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

  private async setServerStatus(id: string, status: McpServerStatus, error?: string | null): Promise<void> {
    await this.repository.updateServer(id, {
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
