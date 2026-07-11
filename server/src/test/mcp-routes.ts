import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Hono } from 'hono'
import { authMiddleware } from '../middleware/auth.js'
import { createRateLimiter } from '../lib/rate-limit.js'
import { initRuntime, shutdownRuntime, type RuntimeContext } from '../lib/runtime.js'
import type { AppConfig } from '../lib/config.js'
import { mcpRoutes } from '../routes/mcps.js'
import { mcpOAuthCallbackRoutes } from '../routes/mcp-oauth-callback.js'
import { buildOAuthCallbackUrl } from '../mcp/oauth-config.js'
import { hashState } from '../mcp/oauth-provider.js'
import { OAuthMcpFixture } from './fixtures/oauth-mcp.js'

type TestEnv = { Variables: { runtime: RuntimeContext; userId: string } }
const dir = mkdtempSync(join(tmpdir(), 'mcp-routes-'))
const fixture = new OAuthMcpFixture()
let runtime: RuntimeContext | undefined

try {
  await fixture.start()
  const config = {
    nodeEnv: 'test', port: 3001, host: 'localhost', dbDialect: 'sqlite', databaseUrl: join(dir, 'routes.db'),
    defaultModel: 'openrouter:test', publicBaseUrl: 'http://localhost:3001', encryptionKey: 'route-encryption-key',
    agentsDir: './agents', tasksDir: join(dir, 'tasks'), workspaceDir: join(dir, 'workspace'),
    sessionFilesDir: join(dir, 'sessions'), inlineOutputLimitBytes: 32768, workflowsDir: './workflows',
    allowedOrigins: '', trustProxy: false, enableShellTool: false, rateLimitAuthFailurePerMin: 120,
    rateLimitApiPerMin: 200, rateLimitInferencePerMin: 60, rateLimitTelegramPerMin: 600,
    rateLimitHealthPerMin: 20, rateLimitOAuthCallbackPerMin: 100,
    langfuseBaseUrl: 'https://cloud.langfuse.com', langfuseCaptureContent: false, langfuseMaxContentChars: 20000,
  } satisfies AppConfig
  runtime = await initRuntime(config)

  const keyA = 'route-user-a'
  const keyB = 'route-user-b'
  const userA = await runtime.repositories.users.create({ email: 'route-a@test', apiKeyHash: sha256(keyA) })
  await runtime.repositories.users.create({ email: 'route-b@test', apiKeyHash: sha256(keyB) })
  const app = testApp(runtime, 100)

  assert.equal((await app.request('/api/mcps')).status, 401)
  assert.equal((await app.request('/oauth/mcp/callback', { headers: { Authorization: 'Bearer invalid' } })).status, 400)

  const invalidCreate = await api(app, keyA, '/api/mcps', { method: 'POST', body: JSON.stringify({ name: '' }) })
  assert.equal(invalidCreate.status, 400)
  const create = await api(app, keyA, '/api/mcps', {
    method: 'POST', body: JSON.stringify({
      name: 'Route OAuth', transport: 'streamable_http', url: fixture.resourceUrl, authMode: 'oauth', enabled: false,
    }),
  })
  assert.equal(create.status, 201)
  const created = await create.json() as any
  assert.equal(created.authStatus, 'required')
  assertNoSecrets(created)
  const serverId = created.id as string

  assert.deepEqual(await (await api(app, keyB, '/api/mcps')).json(), [])
  for (const [method, path] of [
    ['PATCH', `/api/mcps/${serverId}`],
    ['DELETE', `/api/mcps/${serverId}`],
    ['POST', `/api/mcps/${serverId}/connect`],
    ['POST', `/api/mcps/${serverId}/reconnect`],
    ['POST', `/api/mcps/${serverId}/disconnect`],
    ['POST', `/api/mcps/${serverId}/oauth/start`],
    ['GET', `/api/mcps/${serverId}/oauth/session`],
    ['POST', `/api/mcps/${serverId}/oauth/cancel`],
    ['PATCH', `/api/mcps/${serverId}/tools/ping`],
  ] as const) {
    const response = await api(app, keyB, path, {
      method,
      ...(method === 'PATCH' ? { body: JSON.stringify(path.includes('/tools/') ? { enabledForNewSessions: false } : { name: 'stolen' }) } : {}),
    })
    assert.equal(response.status, 404, `${method} ${path} must hide wrong-owner records`)
  }

  const start = await api(app, keyA, `/api/mcps/${serverId}/oauth/start`, { method: 'POST' })
  assert.equal(start.status, 200)
  const startBody = await start.json() as any
  assert.equal(startBody.session.status, 'pending')
  assertNoSecrets(startBody)
  const approval = fixture.approve(startBody.session.authorizationUrl)
  const callback = await app.request(`/oauth/mcp/callback?code=${encodeURIComponent(approval.code)}&state=${encodeURIComponent(approval.state)}`)
  assert.equal(callback.status, 200)
  assertCallbackHeaders(callback)
  const callbackBody = await callback.text()
  assert(!callbackBody.includes(approval.code) && !callbackBody.includes(approval.state))

  const ready = (await api(app, keyA, '/api/mcps')).json() as Promise<any[]>
  const readyServer = (await ready).find((server) => server.id === serverId)
  assert.equal(readyServer.connectionStatus, 'connected')
  assert.equal(readyServer.authStatus, 'authorized')
  assert.equal(readyServer.tools[0].remoteName, 'ping')
  assertNoSecrets(readyServer)

  const toolToggle = await api(app, keyA, `/api/mcps/${serverId}/tools/ping`, {
    method: 'PATCH', body: JSON.stringify({ enabledForNewSessions: false }),
  })
  assert.equal(toolToggle.status, 200)
  assert.equal((await toolToggle.json() as any).enabledForNewSessions, false)
  const sessionStatus = await api(app, keyA, `/api/mcps/${serverId}/oauth/session`)
  assert.equal((await sessionStatus.json() as any).session.status, 'authorized')

  assert.equal((await app.request(`/oauth/mcp/callback?code=${approval.code}&state=${approval.state}`)).status, 410)
  assert.equal((await app.request('/oauth/mcp/callback?state=unknown&code=unknown')).status, 410)

  await api(app, keyA, `/api/mcps/${serverId}/disconnect`, { method: 'POST' })
  const secondStart = await api(app, keyA, `/api/mcps/${serverId}/oauth/start`, { method: 'POST' })
  const second = await secondStart.json() as any
  const secondApproval = fixture.approve(second.session.authorizationUrl)
  const bearerIgnored = await app.request(
    `/oauth/mcp/callback?code=${secondApproval.code}&state=${secondApproval.state}&serverId=wrong&userId=wrong`,
    { headers: { Authorization: 'Bearer invalid-application-token' } },
  )
  assert.equal(bearerIgnored.status, 200)

  await api(app, keyA, `/api/mcps/${serverId}/disconnect`, { method: 'POST' })
  const denialStart = await api(app, keyA, `/api/mcps/${serverId}/oauth/start`, { method: 'POST' })
  const denial = fixture.approve((await denialStart.json() as any).session.authorizationUrl)
  const denied = await app.request(`/oauth/mcp/callback?error=access_denied&state=${denial.state}`)
  assert.equal(denied.status, 400)
  assert(!((await denied.text()).includes(denial.state)))
  assert.equal((await (await api(app, keyA, `/api/mcps/${serverId}/oauth/session`)).json() as any).session.status, 'denied')

  const cancelStart = await api(app, keyA, `/api/mcps/${serverId}/oauth/start`, { method: 'POST' })
  const cancel = fixture.approve((await cancelStart.json() as any).session.authorizationUrl)
  assert.equal((await api(app, keyA, `/api/mcps/${serverId}/oauth/cancel`, { method: 'POST' })).status, 200)
  assert.equal((await app.request(`/oauth/mcp/callback?code=${cancel.code}&state=${cancel.state}`)).status, 410)

  const expiredState = 'expired-state'
  const expiredNow = Date.now()
  await runtime.repositories.mcp.createOAuthSession({
    id: 'expired-session', serverId, userId: userA.id, stateHash: hashState(expiredState), codeVerifier: 'expired-verifier',
    status: 'pending', error: null, expiresAt: expiredNow - 1, consumedAt: null, createdAt: expiredNow, updatedAt: expiredNow,
  })
  assert.equal((await app.request(`/oauth/mcp/callback?code=expired-code&state=${expiredState}`)).status, 410)
  assert.equal((await runtime.repositories.mcp.getOAuthSession(userA.id, serverId, 'expired-session'))?.status, 'expired')

  const malformedStart = await api(app, keyA, `/api/mcps/${serverId}/oauth/start`, { method: 'POST' })
  const malformed = fixture.approve((await malformedStart.json() as any).session.authorizationUrl)
  assert.equal((await app.request(`/oauth/mcp/callback?code=${malformed.code}&error=access_denied&state=${malformed.state}`)).status, 400)
  assert.equal((await app.request(`/oauth/mcp/callback?code=${malformed.code}&state=${malformed.state}`)).status, 200)

  const noAuthCreate = await api(app, keyA, '/api/mcps', {
    method: 'POST', body: JSON.stringify({ name: 'No auth route', transport: 'streamable_http', url: fixture.noAuthUrl, authMode: 'none' }),
  })
  const noAuth = await noAuthCreate.json() as any
  assert.equal((await api(app, keyA, `/api/mcps/${noAuth.id}/connect`, { method: 'POST' })).status, 200)
  assert.equal((await api(app, keyA, `/api/mcps/${noAuth.id}/disconnect`, { method: 'POST' })).status, 200)
  assert.equal((await api(app, keyA, `/api/mcps/${noAuth.id}/reconnect`, { method: 'POST' })).status, 200)
  assert.equal((await api(app, keyA, `/api/mcps/${noAuth.id}`, { method: 'DELETE' })).status, 200)

  const limited = testApp(runtime, 2)
  assert.notEqual((await limited.request('/oauth/mcp/callback')).status, 429)
  assert.notEqual((await limited.request('/oauth/mcp/callback')).status, 429)
  assert.equal((await limited.request('/oauth/mcp/callback')).status, 429)

  const apiLimited = testApp(runtime, 100, 2)
  assert.notEqual((await api(apiLimited, keyA, '/api/mcps')).status, 429)
  assert.notEqual((await api(apiLimited, keyA, '/api/mcps')).status, 429)
  assert.equal((await api(apiLimited, keyA, '/api/mcps')).status, 429)

  assert.equal(buildOAuthCallbackUrl('http://localhost:3001', 'development'), 'http://localhost:3001/oauth/mcp/callback')
  assert.equal(buildOAuthCallbackUrl('https://api.example.com', 'production'), 'https://api.example.com/oauth/mcp/callback')
  assert.throws(() => buildOAuthCallbackUrl('http://api.example.com', 'production'), /HTTPS/)
  assert.throws(() => buildOAuthCallbackUrl('http://api.example.com', 'development'), /loopback HTTP/)
  assert.throws(() => buildOAuthCallbackUrl('https://api.example.com/base', 'production'), /origin/)

  console.log('MCP lifecycle route and public callback tests passed')
} finally {
  if (runtime) await shutdownRuntime(runtime)
  await fixture.stop()
  rmSync(dir, { recursive: true, force: true })
}

function testApp(runtime: RuntimeContext, callbackLimit: number, apiLimit = 1_000): Hono<TestEnv> {
  const app = new Hono<TestEnv>()
  app.use('*', async (c, next) => { c.set('runtime', runtime); await next() })
  app.use('/oauth/mcp/*', createRateLimiter({ name: `callback-test-${callbackLimit}`, limit: callbackLimit, keyBy: 'ip', trustProxy: false }))
  app.route('/oauth/mcp', mcpOAuthCallbackRoutes(runtime))
  app.use('/api/*', authMiddleware)
  app.use('/api/*', createRateLimiter({ name: `api-test-${apiLimit}`, limit: apiLimit, keyBy: 'user', trustProxy: false }))
  app.route('/api/mcps', mcpRoutes(runtime))
  return app
}

function api(app: Hono<TestEnv>, key: string, path: string, init: RequestInit = {}) {
  const headers = new Headers(init.headers)
  headers.set('Authorization', `Bearer ${key}`)
  if (init.body) headers.set('Content-Type', 'application/json')
  return app.request(path, { ...init, headers })
}

function assertNoSecrets(value: unknown): void {
  const json = JSON.stringify(value)
  for (const field of ['access_token', 'refresh_token', 'client_secret', 'code_verifier', 'stateHash', 'state_hash']) {
    assert(!json.includes(field), `API response exposed secret-shaped field ${field}`)
  }
}

function assertCallbackHeaders(response: Response): void {
  assert.match(response.headers.get('content-security-policy') ?? '', /default-src 'none'/)
  assert.match(response.headers.get('cache-control') ?? '', /no-store/)
  assert.equal(response.headers.get('referrer-policy'), 'no-referrer')
  assert.equal(response.headers.get('x-content-type-options'), 'nosniff')
}

function sha256(value: string): string {
  return createHash('sha256').update(value).digest('hex')
}
