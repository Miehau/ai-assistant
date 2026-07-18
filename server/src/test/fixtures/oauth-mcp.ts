import { createHash, randomBytes } from 'node:crypto'
import type { Server as HttpServer } from 'node:http'
import { serve } from '@hono/node-server'
import { Hono } from 'hono'
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { WebStandardStreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js'
import { z } from 'zod'

type Authorization = { challenge: string; redirectUri: string; clientId: string; resource: string }

export class OAuthMcpFixture {
  private server?: HttpServer
  private base = ''
  private readonly codes = new Map<string, Authorization>()
  private readonly accessTokens = new Set<string>()
  private readonly refreshTokens = new Set<string>()
  private refreshCounter = 0
  private rejectRegistration = false
  private advertisedResource: string | null = null

  async start(): Promise<void> {
    const app = new Hono()
    app.get('/.well-known/oauth-protected-resource/mcp', (c) => c.json({
      resource: this.advertisedResource ?? `${this.base}/mcp`, authorization_servers: [this.base], scopes_supported: ['mcp:tools'],
    }))
    app.get('/.well-known/oauth-authorization-server', (c) => c.json({
      issuer: this.base,
      authorization_endpoint: `${this.base}/authorize`,
      token_endpoint: `${this.base}/token`,
      registration_endpoint: `${this.base}/register`,
      response_types_supported: ['code'],
      grant_types_supported: ['authorization_code', 'refresh_token'],
      token_endpoint_auth_methods_supported: ['none'],
      code_challenge_methods_supported: ['S256'],
      scopes_supported: ['mcp:tools'],
    }))
    app.post('/register', async (c) => {
      if (this.rejectRegistration) return c.json({ error: 'invalid_client_metadata' }, 400)
      const body = await c.req.json<Record<string, unknown>>()
      return c.json({ ...body, client_id: `client-${randomBytes(8).toString('hex')}`, client_id_issued_at: 1 })
    })
    app.get('/authorize', (c) => c.text('fixture authorization'))
    app.post('/token', async (c) => {
      const form = new URLSearchParams(await c.req.text())
      const grantType = form.get('grant_type')
      if (grantType === 'authorization_code') {
        const code = form.get('code') ?? ''
        const authorization = this.codes.get(code)
        const verifier = form.get('code_verifier') ?? ''
        if (!authorization || pkceChallenge(verifier) !== authorization.challenge || form.get('redirect_uri') !== authorization.redirectUri || form.get('client_id') !== authorization.clientId || form.get('resource') !== authorization.resource) {
          return c.json({ error: 'invalid_grant' }, 400)
        }
        this.codes.delete(code)
        return c.json(this.issueTokens())
      }
      if (grantType === 'refresh_token') {
        const refresh = form.get('refresh_token') ?? ''
        if (!this.refreshTokens.delete(refresh)) return c.json({ error: 'invalid_grant' }, 400)
        return c.json(this.issueTokens())
      }
      return c.json({ error: 'unsupported_grant_type' }, 400)
    })
    app.all('/mcp', async (c) => {
      if (!this.accessTokens.has(bearer(c.req.header('Authorization')))) return this.unauthorized(c)
      return this.handleMcp(c.req.raw)
    })
    app.all('/mcp-no-auth', (c) => this.handleMcp(c.req.raw))
    app.all('/mcp-bearer', async (c) => {
      if (bearer(c.req.header('Authorization')) !== 'fixture-static-bearer') return c.text('Unauthorized', 401)
      return this.handleMcp(c.req.raw)
    })
    this.server = serve({ fetch: app.fetch, port: 0 }) as HttpServer
    await new Promise<void>((resolve) => this.server!.once('listening', resolve))
    const address = this.server.address()
    if (!address || typeof address === 'string') throw new Error('Fixture did not bind a TCP port')
    this.base = `http://127.0.0.1:${address.port}`
  }

  get resourceUrl(): string { return `${this.base}/mcp` }
  get noAuthUrl(): string { return `${this.base}/mcp-no-auth` }
  get bearerUrl(): string { return `${this.base}/mcp-bearer` }
  get publicBaseUrl(): string { return this.base }

  approve(authorizationUrl: string): { code: string; state: string } {
    const url = new URL(authorizationUrl)
    const state = required(url.searchParams.get('state'))
    const code = `code-${randomBytes(8).toString('hex')}`
    this.codes.set(code, {
      challenge: required(url.searchParams.get('code_challenge')),
      redirectUri: required(url.searchParams.get('redirect_uri')),
      clientId: required(url.searchParams.get('client_id')),
      resource: required(url.searchParams.get('resource')),
    })
    return { code, state }
  }

  expireAccessTokens(): void { this.accessTokens.clear() }
  invalidateRefreshTokens(): void { this.refreshTokens.clear() }
  setRejectRegistration(value: boolean): void { this.rejectRegistration = value }
  setAdvertisedResource(value: string | null): void { this.advertisedResource = value }

  async stop(): Promise<void> {
    if (!this.server) return
    this.server.closeAllConnections()
    await new Promise<void>((resolve, reject) => this.server!.close((error) => error ? reject(error) : resolve()))
    this.server = undefined
  }

  private issueTokens() {
    const access_token = `access-${randomBytes(8).toString('hex')}`
    const refresh_token = `refresh-${++this.refreshCounter}-${randomBytes(8).toString('hex')}`
    this.accessTokens.add(access_token)
    this.refreshTokens.add(refresh_token)
    return { access_token, refresh_token, token_type: 'bearer', expires_in: 1, scope: 'mcp:tools' }
  }

  private unauthorized(c: any) {
    c.header('WWW-Authenticate', `Bearer resource_metadata="${this.base}/.well-known/oauth-protected-resource/mcp", scope="mcp:tools"`)
    return c.text('Unauthorized', 401)
  }

  private async handleMcp(request: Request): Promise<Response> {
    const server = new McpServer({ name: 'oauth-fixture', version: '1.0.0' })
    server.registerTool('ping', {
      description: 'Return a deterministic fixture response', inputSchema: { value: z.string().optional() },
    }, async ({ value }) => ({
      content: [{ type: 'text', text: value ? `pong:${value}` : 'pong' }],
      ...(value === 'structured' ? { structuredContent: { checklistUrl: 'https://meals.example/checklist/test' } } : {}),
    }))
    const transport = new WebStandardStreamableHTTPServerTransport()
    await server.connect(transport)
    return transport.handleRequest(request)
  }
}

function bearer(value?: string): string {
  return value?.startsWith('Bearer ') ? value.slice(7) : ''
}

function pkceChallenge(verifier: string): string {
  return createHash('sha256').update(verifier).digest('base64url')
}

function required(value: string | null): string {
  if (!value) throw new Error('Fixture authorization URL is missing a required value')
  return value
}
