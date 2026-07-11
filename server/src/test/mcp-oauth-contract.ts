import assert from 'node:assert/strict'
import { transitionMcpConnection } from '../mcp/oauth-contract.js'
import type { McpConnectionState } from '../mcp/types.js'

const base = {
  authMode: 'auto',
  authStatus: 'required',
  connectionStatus: 'error',
  oauthCredentialsConfigured: false,
  oauthSession: null,
  error: null,
} satisfies McpConnectionState

assert.deepEqual(transitionMcpConnection(base, 'connect_no_auth'), {
  ...base, authStatus: 'not_required', connectionStatus: 'connected',
})
assert.equal(transitionMcpConnection(base, 'authorization_required').authStatus, 'required')
assert.equal(transitionMcpConnection(base, 'authorization_started').authStatus, 'pending')
assert.deepEqual(transitionMcpConnection(base, 'callback_succeeded'), {
  ...base, authStatus: 'authorized', connectionStatus: 'connected', oauthCredentialsConfigured: true,
})
for (const event of ['authorization_denied', 'session_expired'] as const) {
  const state = transitionMcpConnection({ ...base, authStatus: 'pending' }, event)
  assert.equal(state.authStatus, 'required')
  assert.equal(state.oauthCredentialsConfigured, false)
}
assert.deepEqual(transitionMcpConnection({ ...base, authStatus: 'authorized' }, 'state_replayed'), {
  ...base, authStatus: 'authorized',
})
assert.equal(transitionMcpConnection({ ...base, authStatus: 'authorized' }, 'refresh_failed').authStatus, 'required')

console.log('MCP OAuth contract tests passed')
