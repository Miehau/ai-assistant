import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createDatabase, SQLiteRepositories } from '../repositories/sqlite/index.js'
import { ToolRegistryImpl } from '../tools/registry.js'
import { McpManager } from '../mcp/manager.js'
import { hashState } from '../mcp/oauth-provider.js'
import { OAuthMcpFixture } from './fixtures/oauth-mcp.js'

const fixture = new OAuthMcpFixture()
const dir = mkdtempSync(join(tmpdir(), 'mcp-oauth-integration-'))
try {
  await fixture.start()
  const db = createDatabase(join(dir, 'integration.db'))
  const repos = new SQLiteRepositories(db, 'integration-encryption-key')
  const user = await repos.users.create({ email: 'oauth@test', apiKeyHash: 'oauth-hash' })
  const otherUser = await repos.users.create({ email: 'other@test', apiKeyHash: 'other-hash' })

  let registry = new ToolRegistryImpl()
  let manager = new McpManager(repos.mcp, repos.sessions, registry, 'integration-encryption-key', fixture.publicBaseUrl)
  const oauth = await manager.createServer(user.id, {
    name: 'OAuth fixture', transport: 'streamable_http', url: fixture.resourceUrl, authMode: 'oauth', enabled: false,
  })

  const pending = await manager.startOAuth(user.id, oauth.id)
  assert(pending)
  const beforeCallback = await repos.mcp.getOAuthCredentials(user.id, oauth.id)
  assert(beforeCallback?.clientInformation && beforeCallback.discovery && !beforeCallback.tokens)
  const approval = fixture.approve(pending.authorizationUrl)
  const consumed = await repos.mcp.consumeOAuthSession(hashState(approval.state), Date.now())
  assert.equal(consumed?.id, pending.sessionId)
  const connected = await manager.finishOAuth(user.id, oauth.id, pending.sessionId, approval.code)
  assert.equal(connected.connectionStatus, 'connected')
  assert.equal(connected.authStatus, 'authorized')
  assert.equal(connected.tools[0]?.remoteName, 'ping')
  const selectedTools = await manager.getNewSessionToolSnapshot(user.id, [oauth.id])
  assert.equal(selectedTools[0]?.name, connected.tools[0]?.registeredName)
  assert.equal((await manager.getNewSessionToolSnapshot(user.id))[0]?.name, connected.tools[0]?.registeredName)
  assert.deepEqual(await manager.getNewSessionToolSnapshot(user.id, []), [])
  const now = Date.now()
  await repos.mcp.createTool(user.id, {
    id: 'shopping-list-tool',
    serverId: oauth.id,
    remoteName: 'list_shopping_items',
    registeredName: 'mcp.Meal_Minder.list_shopping_items',
    description: 'List shopping items',
    inputSchema: '{"type":"object"}',
    enabledForNewSessions: true,
    createdAt: now,
    updatedAt: now,
  })
  const shoppingTool = (await manager.getNewSessionToolSnapshot(user.id))
    .find(tool => tool.name === 'mcp.Meal_Minder.list_shopping_items')
  assert.equal(shoppingTool?.requires_approval, false)
  assert.equal(selectedTools[0]?.requires_approval, true)

  const chat = await repos.sessions.create({ userId: user.id, title: 'OAuth tool call' })
  const call = await registry.execute(connected.tools[0].registeredName, { value: 'oauth' }, {
    agent_id: 'agent', session_id: chat.id, signal: new AbortController().signal,
  })
  assert.equal(call.ok, true)
  assert.match(String(call.output), /pong:oauth/)
  const structuredCall = await registry.execute(connected.tools[0].registeredName, { value: 'structured' }, {
    agent_id: 'agent', session_id: chat.id, signal: new AbortController().signal,
  })
  assert.deepEqual(structuredCall.output, {
    text: 'pong:structured',
    structuredContent: { checklistUrl: 'https://meals.example/checklist/test' },
  })
  assert.equal(await manager.getServer(otherUser.id, oauth.id), null)
  assert.deepEqual(await manager.getNewSessionToolSnapshot(otherUser.id, [oauth.id]), [])
  await assert.rejects(() => manager.startOAuth(otherUser.id, oauth.id), /not found/)
  const otherChat = await repos.sessions.create({ userId: otherUser.id, title: 'Cross-user denial' })
  const crossUserCall = await registry.execute(connected.tools[0].registeredName, { value: 'forbidden' }, {
    agent_id: 'agent', session_id: otherChat.id, signal: new AbortController().signal,
  })
  assert.equal(crossUserCall.ok, false)
  assert.match(crossUserCall.error ?? '', /unavailable/)

  const noAuth = await manager.createServer(user.id, {
    name: 'No auth fixture', transport: 'streamable_http', url: fixture.noAuthUrl, authMode: 'none', enabled: true,
  })
  assert.equal(noAuth.connectionStatus, 'connected')
  assert.equal(noAuth.authStatus, 'not_required')
  const noAuthCall = await registry.execute(noAuth.tools[0].registeredName, { value: 'no-auth' }, {
    agent_id: 'agent', session_id: chat.id, signal: new AbortController().signal,
  })
  assert.equal(noAuthCall.ok, true)
  assert.match(String(noAuthCall.output), /pong:no-auth/)
  const bearer = await manager.createServer(user.id, {
    name: 'Bearer fixture', transport: 'streamable_http', url: fixture.bearerUrl, authMode: 'bearer',
    bearerToken: 'fixture-static-bearer', enabled: true,
  })
  assert.equal(bearer.connectionStatus, 'connected')
  const bearerCall = await registry.execute(bearer.tools[0].registeredName, { value: 'bearer' }, {
    agent_id: 'agent', session_id: chat.id, signal: new AbortController().signal,
  })
  assert.equal(bearerCall.ok, true)
  assert.match(String(bearerCall.output), /pong:bearer/)
  const stdio = await manager.createServer(user.id, {
    name: 'Stdio fixture', transport: 'stdio', command: process.execPath,
    args: ['--import', 'tsx', fileURLToPath(new URL('./fixtures/stdio-mcp-server.ts', import.meta.url))],
    authMode: 'none', enabled: true,
  })
  assert.equal(stdio.connectionStatus, 'connected')
  const stdioCall = await registry.execute(stdio.tools[0].registeredName, { value: 'stdio' }, {
    agent_id: 'agent', session_id: chat.id, signal: new AbortController().signal,
  })
  assert.equal(stdioCall.ok, true)
  assert.match(String(stdioCall.output), /pong:stdio/)

  const firstTokens = JSON.parse((await repos.mcp.getOAuthCredentials(user.id, oauth.id))!.tokens!) as { refresh_token: string }
  await manager.shutdown()
  fixture.expireAccessTokens()
  registry = new ToolRegistryImpl()
  manager = new McpManager(repos.mcp, repos.sessions, registry, 'integration-encryption-key', fixture.publicBaseUrl)
  await manager.initialize([user.id])
  const afterRestart = await manager.getServer(user.id, oauth.id)
  assert.equal(afterRestart?.connectionStatus, 'connected')
  const rotatedTokens = JSON.parse((await repos.mcp.getOAuthCredentials(user.id, oauth.id))!.tokens!) as { refresh_token: string }
  assert.notEqual(rotatedTokens.refresh_token, firstTokens.refresh_token)

  await manager.disconnectServer(user.id, oauth.id, true)
  const disconnectedCall = await registry.execute(connected.tools[0].registeredName, { value: 'offline' }, {
    agent_id: 'agent', session_id: chat.id, signal: new AbortController().signal,
  })
  assert.equal(disconnectedCall.ok, false)
  assert.match(disconnectedCall.error ?? '', /not connected/)
  const deniedPending = await manager.startOAuth(user.id, oauth.id)
  assert(deniedPending)
  const denied = await manager.cancelOAuth(user.id, oauth.id)
  assert.equal(denied.authStatus, 'required')
  assert.equal(denied.oauthSession?.status, 'cancelled')

  const invalidPending = await manager.startOAuth(user.id, oauth.id)
  assert(invalidPending)
  const invalidApproval = fixture.approve(invalidPending.authorizationUrl)
  await repos.mcp.consumeOAuthSession(hashState(invalidApproval.state), Date.now())
  await assert.rejects(() => manager.finishOAuth(user.id, oauth.id, invalidPending.sessionId, 'invalid-code'), /Could not connect/)
  assert.equal((await manager.getServer(user.id, oauth.id))?.authStatus, 'error')

  const recoveryPending = await manager.startOAuth(user.id, oauth.id)
  assert(recoveryPending)
  const recovery = fixture.approve(recoveryPending.authorizationUrl)
  await repos.mcp.consumeOAuthSession(hashState(recovery.state), Date.now())
  await manager.finishOAuth(user.id, oauth.id, recoveryPending.sessionId, recovery.code)

  const badRegistration = await manager.createServer(user.id, {
    name: 'Bad registration', transport: 'streamable_http', url: fixture.resourceUrl, authMode: 'oauth', enabled: false,
  })
  fixture.setRejectRegistration(true)
  assert.equal(await manager.startOAuth(user.id, badRegistration.id), null)
  assert.equal((await manager.getServer(user.id, badRegistration.id))?.connectionStatus, 'error')
  fixture.setRejectRegistration(false)

  const mismatch = await manager.createServer(user.id, {
    name: 'Resource mismatch', transport: 'streamable_http', url: fixture.resourceUrl, authMode: 'oauth', enabled: false,
  })
  fixture.setAdvertisedResource(`${fixture.publicBaseUrl}/wrong-resource`)
  assert.equal(await manager.startOAuth(user.id, mismatch.id), null)
  assert.match((await manager.getServer(user.id, mismatch.id))?.error ?? '', /resource does not match/)
  fixture.setAdvertisedResource(null)

  await manager.shutdown()
  fixture.expireAccessTokens()
  fixture.invalidateRefreshTokens()
  registry = new ToolRegistryImpl()
  manager = new McpManager(repos.mcp, repos.sessions, registry, 'integration-encryption-key', fixture.publicBaseUrl)
  await manager.initialize([user.id])
  const refreshFailure = await manager.getServer(user.id, oauth.id)
  assert.equal(refreshFailure?.connectionStatus, 'error')
  assert.equal(refreshFailure?.authStatus, 'pending')
  assert.equal(refreshFailure?.oauthCredentialsConfigured, false)

  await manager.shutdown()
  console.log('MCP OAuth manager integration and transport regression tests passed')
} finally {
  await fixture.stop()
  rmSync(dir, { recursive: true, force: true })
}
