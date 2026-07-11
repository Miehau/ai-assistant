import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import Database from 'better-sqlite3'
import { createDatabase, SQLiteRepositories } from '../repositories/sqlite/index.js'
import { ensurePgSchema } from '../repositories/postgres/index.js'
import { McpManager } from '../mcp/manager.js'
import { ToolRegistryImpl } from '../tools/registry.js'

const dir = mkdtempSync(join(tmpdir(), 'mcp-repo-'))
const path = join(dir, 'migration.db')
const old = new Database(path)
old.exec(`
  CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT, api_key_hash TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
  INSERT INTO users VALUES ('user-a', NULL, 'hash-a', 1, 1);
  INSERT INTO users VALUES ('user-b', NULL, 'hash-b', 2, 2);
  CREATE TABLE mcp_servers (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, transport TEXT NOT NULL, command TEXT, args TEXT, env TEXT,
    cwd TEXT, url TEXT, bearer_token TEXT, enabled INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL DEFAULT 'disabled',
    error TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
  );
  CREATE TABLE mcp_tools (
    id TEXT PRIMARY KEY, server_id TEXT NOT NULL REFERENCES mcp_servers(id), remote_name TEXT NOT NULL,
    registered_name TEXT NOT NULL, description TEXT, input_schema TEXT, enabled_for_new_sessions INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
  );
  INSERT INTO mcp_servers VALUES ('legacy-none', 'No auth', 'streamable_http', NULL, '[]', '{}', NULL, 'https://none.test/mcp', NULL, 0, 'disabled', NULL, 1, 1);
  INSERT INTO mcp_servers VALUES ('legacy-bearer', 'Bearer', 'streamable_http', NULL, '[]', '{}', NULL, 'https://bearer.test/mcp', 'legacy-token', 0, 'disabled', NULL, 1, 1);
  INSERT INTO mcp_servers VALUES ('legacy-stdio', 'Local', 'stdio', 'node', '[]', '{}', NULL, NULL, NULL, 0, 'disabled', NULL, 1, 1);
`)
old.close()

try {
  const db = createDatabase(path)
  const repos = new SQLiteRepositories(db, 'test-encryption-key')
  const migrated = await repos.mcp.listServers('user-a')
  assert.deepEqual(migrated.map((server) => [server.id, server.authMode]), [
    ['legacy-bearer', 'bearer'], ['legacy-stdio', 'none'], ['legacy-none', 'auto'],
  ])
  assert.equal((await repos.mcp.listServers('user-b')).length, 0)
  assert.equal(await repos.mcp.getServer('user-b', 'legacy-none'), null)
  assert.equal(await repos.mcp.updateServer('user-b', 'legacy-none', { name: 'stolen' }), false)
  assert.equal(await repos.mcp.deleteServerAndTools('user-b', 'legacy-none'), false)

  const now = Date.now()
  await repos.mcp.createServer({
    id: 'owned', userId: 'user-a', authMode: 'oauth', name: 'OAuth', transport: 'streamable_http', command: null,
    args: '[]', env: '{}', cwd: null, url: 'https://oauth.test/mcp', bearerToken: null, enabled: false,
    status: 'disabled', error: null, createdAt: now, updatedAt: now,
  })
  assert.equal(await repos.mcp.createTool('user-b', {
    id: 'stolen-tool', serverId: 'owned', remoteName: 'steal', registeredName: 'mcp.oauth.steal', description: '',
    inputSchema: '{}', enabledForNewSessions: true, createdAt: now, updatedAt: now,
  }), false)
  assert.equal(await repos.mcp.createTool('user-a', {
    id: 'tool-a', serverId: 'owned', remoteName: 'ping', registeredName: 'mcp.oauth.ping', description: '',
    inputSchema: '{}', enabledForNewSessions: true, createdAt: now, updatedAt: now,
  }), true)
  assert.equal((await repos.mcp.listEnabledServers('user-b')).length, 0)
  assert.equal((await repos.mcp.listTools('user-a')).length, 1)
  assert.equal((await repos.mcp.listTools('user-b')).length, 0)
  assert.equal(await repos.mcp.getTool('user-b', 'tool-a'), null)
  assert.equal(await repos.mcp.getToolByServerAndRemoteName('user-b', 'owned', 'ping'), null)
  assert.equal(await repos.mcp.updateTool('user-b', 'tool-a', { enabledForNewSessions: false }), false)
  await repos.mcp.deleteToolsForServer('user-b', 'owned')
  assert.equal((await repos.mcp.listTools('user-a')).length, 1)

  await repos.mcp.saveOAuthCredentials({
    serverId: 'owned', userId: 'user-a', resourceUrl: 'https://oauth.test/mcp',
    tokens: 'raw-access-and-refresh-token', clientInformation: 'raw-client-secret', discovery: 'raw-discovery', updatedAt: now,
  })
  await repos.mcp.createOAuthSession({
    id: 'pending', serverId: 'owned', userId: 'user-a', stateHash: 'hashed-state', codeVerifier: 'raw-verifier',
    status: 'pending', error: null, expiresAt: now + 10_000, consumedAt: null, createdAt: now, updatedAt: now,
  })
  assert.equal(await repos.mcp.getOAuthCredentials('user-b', 'owned'), null)
  assert.equal(await repos.mcp.getOAuthSession('user-b', 'owned'), null)
  assert.equal(await repos.mcp.deleteOAuthCredentials('user-b', 'owned'), false)
  assert.equal(await repos.mcp.updateOAuthSession('user-b', 'owned', 'pending', { status: 'cancelled' }), false)
  await assert.rejects(() => repos.mcp.saveOAuthCredentials({
    serverId: 'owned', userId: 'user-b', resourceUrl: 'https://oauth.test/mcp', tokens: 'cross-user', updatedAt: now,
  }), /not found/)
  await assert.rejects(() => repos.mcp.createOAuthSession({
    id: 'cross-user', serverId: 'owned', userId: 'user-b', stateHash: 'cross-user-hash', codeVerifier: 'cross-user',
    status: 'pending', error: null, expiresAt: now + 1, consumedAt: null, createdAt: now, updatedAt: now,
  }), /not found/)
  assert.equal((await repos.mcp.getOAuthCredentials('user-a', 'owned'))?.tokens, 'raw-access-and-refresh-token')
  assert.equal((await repos.mcp.getOAuthSession('user-a', 'owned'))?.codeVerifier, 'raw-verifier')

  const raw = new Database(path, { readonly: true })
  const credentialRow = raw.prepare('SELECT tokens, client_information, discovery FROM mcp_oauth_credentials').get() as Record<string, string>
  const sessionRow = raw.prepare('SELECT state_hash, code_verifier FROM mcp_oauth_sessions').get() as Record<string, string>
  assert(Object.values(credentialRow).every((value) => value.startsWith('enc::') && !value.includes('raw-')))
  assert.equal(sessionRow.state_hash, 'hashed-state')
  assert(sessionRow.code_verifier.startsWith('enc::') && !sessionRow.code_verifier.includes('raw-verifier'))
  raw.close()

  const consumed = await repos.mcp.consumeOAuthSession('hashed-state', now + 1)
  assert.equal(consumed?.id, 'pending')
  assert.equal(await repos.mcp.consumeOAuthSession('hashed-state', now + 2), null)
  await repos.mcp.createOAuthSession({
    id: 'expired', serverId: 'owned', userId: 'user-a', stateHash: 'expired-hash', codeVerifier: 'expired-verifier',
    status: 'pending', error: null, expiresAt: now - 1, consumedAt: null, createdAt: now, updatedAt: now,
  })
  assert.equal(await repos.mcp.cleanupExpiredOAuthSessions(now, 1), 1)
  assert.equal((await repos.mcp.getOAuthSession('user-a', 'owned', 'expired'))?.status, 'expired')

  const ownerSession = await repos.sessions.create({ userId: 'user-a', title: 'owner' })
  const otherSession = await repos.sessions.create({ userId: 'user-b', title: 'other' })
  const registry = new ToolRegistryImpl()
  const manager = new McpManager(repos.mcp, repos.sessions, registry, 'test-encryption-key', 'http://localhost:3001')
  await manager.initialize(['user-a', 'user-b'])
  const wrongOwnerCall = await registry.execute('mcp.OAuth_owned.ping', {}, {
    agent_id: 'agent', session_id: otherSession.id, signal: new AbortController().signal,
  })
  assert.deepEqual(wrongOwnerCall, { ok: false, error: 'MCP tool is unavailable for this session' })
  const ownerCall = await registry.execute('mcp.OAuth_owned.ping', {}, {
    agent_id: 'agent', session_id: ownerSession.id, signal: new AbortController().signal,
  })
  assert.equal(ownerCall.ok, false)
  assert.match(ownerCall.error ?? '', /not connected/)
  await manager.shutdown()

  assert.equal(await repos.mcp.deleteServerAndTools('user-a', 'owned'), true)
  assert.equal((await repos.mcp.listTools('user-a')).length, 0)
  assert.equal(await repos.mcp.getOAuthCredentials('user-a', 'owned'), null)
  assert.equal(await repos.mcp.getOAuthSession('user-a', 'owned'), null)

  const withoutEncryption = new SQLiteRepositories(db)
  await assert.rejects(() => withoutEncryption.mcp.saveOAuthCredentials({
    serverId: 'legacy-none', userId: 'user-a', resourceUrl: 'https://none.test/mcp', tokens: 'token', updatedAt: now,
  }), /ENCRYPTION_KEY/)

  const pgStatements: string[] = []
  const fakePg = Object.assign(
    async () => [{ count: 0 }],
    { unsafe: async (statement: string) => { pgStatements.push(statement) } },
  )
  await ensurePgSchema(fakePg as never)
  await ensurePgSchema(fakePg as never)
  const pgSql = pgStatements.join('\n')
  assert(pgSql.includes('mcp_oauth_credentials') && pgSql.includes('mcp_oauth_sessions'))
  assert(pgSql.includes('ADD COLUMN IF NOT EXISTS user_id') && pgSql.includes('ALTER COLUMN user_id SET NOT NULL'))

  console.log('MCP repository migration, ownership, encryption, and cleanup tests passed')
} finally {
  rmSync(dir, { recursive: true, force: true })
}
