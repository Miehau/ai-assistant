import type { ToolMetadata } from '../tools/types.js'

export type McpTransport = 'stdio' | 'streamable_http'
export type McpServerStatus = 'disabled' | 'connected' | 'error'

export interface McpToolRecord {
  id: string
  serverId: string
  remoteName: string
  registeredName: string
  description: string
  inputSchema: Record<string, unknown>
  enabledForNewSessions: boolean
  createdAt: number
  updatedAt: number
}

export interface McpServerRecord {
  id: string
  name: string
  transport: McpTransport
  command: string | null
  args: string[]
  env: Record<string, string>
  cwd: string | null
  url: string | null
  bearerTokenConfigured: boolean
  enabled: boolean
  status: McpServerStatus
  error: string | null
  createdAt: number
  updatedAt: number
  tools: McpToolRecord[]
}

export interface CreateMcpServerInput {
  name: string
  transport: McpTransport
  command?: string | null
  args?: string[]
  env?: Record<string, string>
  cwd?: string | null
  url?: string | null
  bearerToken?: string | null
  enabled?: boolean
}

export interface UpdateMcpServerInput {
  name?: string
  transport?: McpTransport
  command?: string | null
  args?: string[]
  env?: Record<string, string>
  cwd?: string | null
  url?: string | null
  bearerToken?: string | null
  enabled?: boolean
}

export interface McpToolSnapshot extends ToolMetadata {
  source_id: string
}
