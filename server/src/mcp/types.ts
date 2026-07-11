import type { ToolMetadata } from '../tools/types.js'

export type McpTransport = 'stdio' | 'streamable_http'
export type McpServerStatus = 'disabled' | 'connecting' | 'connected' | 'error'
export type McpAuthMode = 'auto' | 'none' | 'bearer' | 'oauth'
export type McpAuthStatus = 'not_required' | 'required' | 'pending' | 'authorized' | 'error'
export type McpConnectionStatus = 'disabled' | 'connecting' | 'connected' | 'error'
export type McpOAuthSessionStatus = 'pending' | 'authorized' | 'denied' | 'expired' | 'cancelled' | 'error'

export interface McpOAuthSessionRecord {
  id: string
  serverId: string
  status: McpOAuthSessionStatus
  authorizationUrl?: string
  expiresAt: number
  error: string | null
}

export interface McpConnectionState {
  authMode: McpAuthMode
  authStatus: McpAuthStatus
  connectionStatus: McpConnectionStatus
  oauthCredentialsConfigured: boolean
  oauthSession: McpOAuthSessionRecord | null
  error: string | null
}

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
  authMode: McpAuthMode
  authStatus: McpAuthStatus
  connectionStatus: McpConnectionStatus
  oauthCredentialsConfigured: boolean
  oauthSession: McpOAuthSessionRecord | null
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
  authMode?: McpAuthMode
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
  authMode?: McpAuthMode
  enabled?: boolean
}

export interface McpToolSnapshot extends ToolMetadata {
  source_id: string
}
