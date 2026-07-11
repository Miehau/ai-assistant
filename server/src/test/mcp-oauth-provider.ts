import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createDatabase, SQLiteRepositories } from '../repositories/sqlite/index.js'
import { PersistentMcpOAuthProvider, canonicalResource, hashState } from '../mcp/oauth-provider.js'

const dir = mkdtempSync(join(tmpdir(), 'mcp-oauth-provider-'))
try {
  const db = createDatabase(join(dir, 'provider.db'))
  const repos = new SQLiteRepositories(db, 'provider-test-key')
  await repos.users.create({ email: 'a@test', apiKeyHash: 'hash-a' })
  await repos.users.create({ email: 'b@test', apiKeyHash: 'hash-b' })
  const [userA, userB] = await repos.users.list()
  const now = Date.now()
  for (const [id, userId, url] of [
    ['server-a', userA.id, 'https://mcp.test/mcp'],
    ['server-b', userB.id, 'https://other.test/mcp'],
  ] as const) {
    await repos.mcp.createServer({
      id, userId, authMode: 'oauth', name: id, transport: 'streamable_http', command: null, args: '[]', env: '{}',
      cwd: null, url, bearerToken: null, enabled: false, status: 'disabled', error: null, createdAt: now, updatedAt: now,
    })
  }

  const redirect = 'http://localhost:3001/oauth/mcp/callback'
  const provider = new PersistentMcpOAuthProvider(repos.mcp, userA.id, 'server-a', 'https://mcp.test/mcp', redirect)
  await provider.saveClientInformation({ client_id: 'client-a', client_secret: 'secret-a' })
  await provider.saveTokens({ access_token: 'access-a', token_type: 'bearer', refresh_token: 'refresh-a' })
  await provider.saveDiscoveryState({
    authorizationServerUrl: 'https://auth.test',
    resourceMetadata: { resource: 'https://mcp.test/mcp', authorization_servers: ['https://auth.test'] },
  })

  const restored = new PersistentMcpOAuthProvider(repos.mcp, userA.id, 'server-a', 'https://mcp.test/mcp', redirect)
  assert.equal((await restored.clientInformation())?.client_id, 'client-a')
  assert.equal((await restored.tokens())?.refresh_token, 'refresh-a')
  assert.equal((await restored.discoveryState())?.authorizationServerUrl, 'https://auth.test')
  await assert.rejects(() => provider.validateResourceURL('https://mcp.test/mcp', 'https://other.test/mcp'), /does not match/)
  assert.equal((await provider.validateResourceURL('https://MCP.test/mcp')).toString(), 'https://mcp.test/mcp')
  assert.throws(() => canonicalResource('https://mcp.test/mcp?token=bad'), /without credentials, query, or fragment/)

  await provider.invalidateCredentials('tokens')
  assert.equal(await provider.tokens(), undefined)
  assert.equal((await provider.clientInformation())?.client_id, 'client-a')
  await provider.invalidateCredentials('client')
  assert.equal(await provider.clientInformation(), undefined)
  assert.equal((await provider.discoveryState())?.authorizationServerUrl, 'https://auth.test')
  await provider.invalidateCredentials('discovery')
  assert.equal(await provider.discoveryState(), undefined)

  const state = await provider.state()
  await provider.saveCodeVerifier('verifier-a')
  const authUrl = new URL('https://auth.test/authorize')
  authUrl.searchParams.set('state', state)
  await provider.redirectToAuthorization(authUrl)
  const pending = provider.takePendingAuthorization()
  assert(pending && pending.authorizationUrl === authUrl.toString())
  assert.equal(provider.takePendingAuthorization(), null)
  const storedSession = await repos.mcp.getOAuthSession(userA.id, 'server-a', pending.sessionId)
  assert(storedSession)
  assert.equal(storedSession.stateHash, hashState(state))
  assert.equal(storedSession.codeVerifier, 'verifier-a')
  const callbackProvider = new PersistentMcpOAuthProvider(
    repos.mcp, userA.id, 'server-a', 'https://mcp.test/mcp', redirect, pending.sessionId,
  )
  assert.equal(await callbackProvider.codeVerifier(), 'verifier-a')
  assert.equal((await repos.mcp.consumeOAuthSession(hashState(state), now + 1))?.id, pending.sessionId)
  assert.equal(await repos.mcp.consumeOAuthSession(hashState(state), now + 2), null)
  await callbackProvider.invalidateCredentials('verifier')
  assert.equal((await repos.mcp.getOAuthSession(userA.id, 'server-a', pending.sessionId))?.status, 'cancelled')

  const providers = [1, 2].map(() => new PersistentMcpOAuthProvider(
    repos.mcp, userA.id, 'server-a', 'https://mcp.test/mcp', redirect,
  ))
  const states = await Promise.all(providers.map((item) => item.state()))
  await Promise.all(providers.map((item, index) => item.saveCodeVerifier(`verifier-${index}`)))
  await Promise.all(providers.map((item, index) => {
    const url = new URL('https://auth.test/authorize')
    url.searchParams.set('state', states[index])
    return item.redirectToAuthorization(url)
  }))
  const pendingRows = (await Promise.all(providers.map((item) => item.takePendingAuthorization())))
    .filter((item): item is NonNullable<typeof item> => Boolean(item))
  assert.equal(pendingRows.length, 2)
  const statuses = await Promise.all(pendingRows.map((item) => repos.mcp.getOAuthSession(userA.id, 'server-a', item.sessionId)))
  assert.deepEqual(statuses.map((item) => item?.status).sort(), ['cancelled', 'pending'])

  const wrongUserProvider = new PersistentMcpOAuthProvider(
    repos.mcp, userB.id, 'server-a', 'https://mcp.test/mcp', redirect,
  )
  await assert.rejects(() => wrongUserProvider.saveTokens({ access_token: 'cross-user', token_type: 'bearer' }), /not found/)
  const wrongServerProvider = new PersistentMcpOAuthProvider(
    repos.mcp, userB.id, 'server-b', 'https://mcp.test/mcp', redirect,
  )
  await assert.rejects(() => wrongServerProvider.saveTokens({ access_token: 'cross-server', token_type: 'bearer' }), /different MCP resource|not found/)

  await provider.invalidateCredentials('all')
  assert.equal(await repos.mcp.getOAuthCredentials(userA.id, 'server-a'), null)
  console.log('Persistent MCP OAuth provider tests passed')
} finally {
  rmSync(dir, { recursive: true, force: true })
}
