import type { McpConnectionState, McpOAuthSessionStatus } from './types.js'

export type McpOAuthEvent =
  | 'connect_no_auth'
  | 'authorization_required'
  | 'authorization_started'
  | 'callback_succeeded'
  | 'authorization_denied'
  | 'session_expired'
  | 'session_cancelled'
  | 'state_replayed'
  | 'refresh_failed'
  | 'disabled'

const terminalSessionStatuses = new Set<McpOAuthSessionStatus>([
  'authorized', 'denied', 'expired', 'cancelled', 'error',
])

export function isTerminalOAuthSession(status: McpOAuthSessionStatus): boolean {
  return terminalSessionStatuses.has(status)
}

/** Executable state contract; persistence and route layers add timestamps/session IDs. */
export function transitionMcpConnection(
  state: McpConnectionState,
  event: McpOAuthEvent,
): McpConnectionState {
  const next = { ...state, oauthSession: state.oauthSession ? { ...state.oauthSession } : null }
  switch (event) {
    case 'connect_no_auth':
      return { ...next, authStatus: 'not_required', connectionStatus: 'connected', error: null }
    case 'authorization_required':
      return { ...next, authStatus: 'required', connectionStatus: 'error', error: null }
    case 'authorization_started':
      return { ...next, authStatus: 'pending', connectionStatus: 'connecting', error: null }
    case 'callback_succeeded':
      return { ...next, authStatus: 'authorized', connectionStatus: 'connected', oauthCredentialsConfigured: true, error: null }
    case 'authorization_denied':
    case 'session_expired':
    case 'session_cancelled':
      return { ...next, authStatus: 'required', connectionStatus: 'error', oauthCredentialsConfigured: false, error: safeError(event) }
    case 'state_replayed':
      return next
    case 'refresh_failed':
      return { ...next, authStatus: 'required', connectionStatus: 'error', oauthCredentialsConfigured: false, error: 'Sign in again to continue.' }
    case 'disabled':
      return { ...next, connectionStatus: 'disabled', error: null }
  }
}

function safeError(event: McpOAuthEvent): string {
  if (event === 'authorization_denied') return 'Authorization was denied.'
  if (event === 'session_expired') return 'Authorization expired. Start again.'
  if (event === 'session_cancelled') return 'Authorization was cancelled.'
  return 'Authorization is no longer valid. Start again.'
}
