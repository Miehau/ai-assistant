import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js'
import { eq, asc, inArray } from 'drizzle-orm'
import { v4 as uuid } from 'uuid'
import { z } from 'zod'
import * as schema from '../db/schema.js'
import type { DrizzleInstance } from '../repositories/sqlite/index.js'
import type { ToolHandler, ToolMetadata, ToolResult } from '../tools/types.js'
import { ToolRegistryImpl } from '../tools/registry.js'
import { decrypt, deriveKey, encrypt } from '../lib/crypto.js'
import { logger } from '../lib/logger.js'
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
    private readonly db: DrizzleInstance,
    private readonly tools: ToolRegistryImpl,
    encryptionKey?: string,
  ) {
    this.encKey = encryptionKey ? deriveKey(encryptionKey) : null
  }

  async initialize(): Promise<void> {
    this.registerKnownTools()
    const enabled = this.db.select().from(schema.mcpServers).where(eq(schema.mcpServers.enabled, 1)).all()
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
    const servers = this.db.select().from(schema.mcpServers).orderBy(asc(schema.mcpServers.name)).all()
    return servers.map((server) => this.toServerRecord(server, this.getToolsForServer(server.id)))
  }

  async getServer(id: string): Promise<McpServerRecord | null> {
    const server = this.db.select().from(schema.mcpServers).where(eq(schema.mcpServers.id, id)).limit(1).all()[0]
    return server ? this.toServerRecord(server, this.getToolsForServer(server.id)) : null
  }

  async createServer(input: unknown): Promise<McpServerRecord> {
    const parsed = createServerSchema.parse(input) satisfies CreateMcpServerInput
    this.validateServerConfig(parsed)
    const now = Date.now()
    const id = uuid()
    this.db.insert(schema.mcpServers).values({
      id,
      name: parsed.name,
      transport: parsed.transport,
      command: parsed.command ?? null,
      args: JSON.stringify(parsed.args ?? []),
      env: JSON.stringify(parsed.env ?? {}),
      cwd: parsed.cwd ?? null,
      url: parsed.url ?? null,
      bearerToken: parsed.bearerToken ? this.storeSecret(parsed.bearerToken) : null,
      enabled: parsed.enabled ? 1 : 0,
      status: parsed.enabled ? 'error' : 'disabled',
      error: null,
      createdAt: now,
      updatedAt: now,
    }).run()

    if (parsed.enabled) {
      await this.connect(id)
    }
    const server = await this.getServer(id)
    if (!server) throw new Error(`MCP server not found after create: ${id}`)
    return server
  }

  async updateServer(id: string, input: unknown): Promise<McpServerRecord> {
    const existing = this.db.select().from(schema.mcpServers).where(eq(schema.mcpServers.id, id)).limit(1).all()[0]
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

    this.db.update(schema.mcpServers)
      .set({
        name: next.name,
        transport: next.transport,
        command: next.command ?? null,
        args: JSON.stringify(next.args ?? []),
        env: JSON.stringify(next.env ?? {}),
        cwd: next.cwd ?? null,
        url: next.url ?? null,
        bearerToken: next.bearerToken ? this.storeSecret(next.bearerToken) : null,
        enabled: next.enabled ? 1 : 0,
        status: next.enabled ? 'error' : 'disabled',
        error: null,
        updatedAt: Date.now(),
      })
      .where(eq(schema.mcpServers.id, id))
      .run()

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
    this.db.delete(schema.mcpTools).where(eq(schema.mcpTools.serverId, id)).run()
    this.db.delete(schema.mcpServers).where(eq(schema.mcpServers.id, id)).run()
    return true
  }

  async setToolEnabled(serverId: string, remoteName: string, enabled: boolean): Promise<McpToolRecord> {
    const tool = this.db
      .select()
      .from(schema.mcpTools)
      .where(eq(schema.mcpTools.serverId, serverId))
      .all()
      .find((row) => row.remoteName === remoteName)
    if (!tool) throw new Error(`MCP tool not found: ${remoteName}`)

    this.db.update(schema.mcpTools)
      .set({ enabledForNewSessions: enabled ? 1 : 0, updatedAt: Date.now() })
      .where(eq(schema.mcpTools.id, tool.id))
      .run()
    const updated = this.db.select().from(schema.mcpTools).where(eq(schema.mcpTools.id, tool.id)).limit(1).all()[0]
    return this.toToolRecord(updated)
  }

  getNewSessionToolSnapshot(serverIds: string[]): McpToolSnapshot[] {
    if (serverIds.length === 0) return []
    const servers = this.db
      .select()
      .from(schema.mcpServers)
      .where(inArray(schema.mcpServers.id, serverIds))
      .all()
      .filter((server) => server.enabled === 1)
    const enabledServerIds = new Set(servers.map((server) => server.id))
    if (enabledServerIds.size === 0) return []

    return this.db.select().from(schema.mcpTools).all()
      .filter((tool) => enabledServerIds.has(tool.serverId) && tool.enabledForNewSessions === 1)
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
    const server = this.db.select().from(schema.mcpServers).where(eq(schema.mcpServers.id, id)).limit(1).all()[0]
    if (!server) throw new Error(`MCP server not found: ${id}`)

    try {
      const client = new Client({ name: 'ai-frontend-server', version: '0.1.0' }, { capabilities: {} })
      const transport = this.createTransport(server)
      await client.connect(transport)
      this.clients.set(id, client)

      const response = await client.listTools()
      const remoteTools = response.tools ?? []
      this.upsertDiscoveredTools(server.id, server.name, remoteTools)
      this.registerKnownTools(server.id)
      this.setServerStatus(id, 'connected', null)
    } catch (err) {
      await this.disconnect(id, 'error')
      const message = err instanceof Error ? err.message : String(err)
      this.setServerStatus(id, 'error', message)
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
    this.setServerStatus(id, status, status === 'disabled' ? null : undefined)
  }

  private createTransport(server: typeof schema.mcpServers.$inferSelect): any {
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

  private registerKnownTools(serverId?: string): void {
    const rows = serverId
      ? this.db.select().from(schema.mcpTools).where(eq(schema.mcpTools.serverId, serverId)).all()
      : this.db.select().from(schema.mcpTools).all()

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

  private upsertDiscoveredTools(
    serverId: string,
    serverName: string,
    tools: Array<{ name: string; description?: string; inputSchema?: Record<string, unknown> }>,
  ): void {
    const now = Date.now()
    const existing = this.db.select().from(schema.mcpTools).where(eq(schema.mcpTools.serverId, serverId)).all()
    const existingByRemoteName = new Map(existing.map((tool) => [tool.remoteName, tool]))

    for (const tool of tools) {
      const current = existingByRemoteName.get(tool.name)
      const registeredName = `mcp.${sanitizeName(serverName)}.${sanitizeName(tool.name)}`
      if (current) {
        this.db.update(schema.mcpTools)
          .set({
            registeredName,
            description: tool.description ?? '',
            inputSchema: JSON.stringify(normalizeSchema(tool.inputSchema)),
            updatedAt: now,
          })
          .where(eq(schema.mcpTools.id, current.id))
          .run()
      } else {
        this.db.insert(schema.mcpTools).values({
          id: uuid(),
          serverId,
          remoteName: tool.name,
          registeredName,
          description: tool.description ?? '',
          inputSchema: JSON.stringify(normalizeSchema(tool.inputSchema)),
          enabledForNewSessions: 1,
          createdAt: now,
          updatedAt: now,
        }).run()
      }
    }
  }

  private getToolsForServer(serverId: string): McpToolRecord[] {
    return this.db
      .select()
      .from(schema.mcpTools)
      .where(eq(schema.mcpTools.serverId, serverId))
      .orderBy(asc(schema.mcpTools.remoteName))
      .all()
      .map((tool) => this.toToolRecord(tool))
  }

  private toServerRecord(
    row: typeof schema.mcpServers.$inferSelect,
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
      enabled: Boolean(row.enabled),
      status: row.status as McpServerStatus,
      error: row.error ?? null,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
      tools,
    }
  }

  private toToolRecord(row: typeof schema.mcpTools.$inferSelect): McpToolRecord {
    return {
      id: row.id,
      serverId: row.serverId,
      remoteName: row.remoteName,
      registeredName: row.registeredName,
      description: row.description ?? '',
      inputSchema: normalizeSchema(parseJson(row.inputSchema, { type: 'object', properties: {} })),
      enabledForNewSessions: Boolean(row.enabledForNewSessions),
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    }
  }

  private setServerStatus(id: string, status: McpServerStatus, error?: string | null): void {
    const updates: Record<string, unknown> = { status, updatedAt: Date.now() }
    if (error !== undefined) updates.error = error
    this.db.update(schema.mcpServers).set(updates).where(eq(schema.mcpServers.id, id)).run()
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
