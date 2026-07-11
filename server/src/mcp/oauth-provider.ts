import { createHash, randomBytes } from 'node:crypto'
import type { OAuthClientProvider, OAuthDiscoveryState } from '@modelcontextprotocol/sdk/client/auth.js'
import type { OAuthClientInformationMixed, OAuthClientMetadata, OAuthTokens } from '@modelcontextprotocol/sdk/shared/auth.js'
import { v4 as uuid } from 'uuid'
import type { McpRepository, StoredMcpOAuthSession } from '../repositories/types.js'

const OAUTH_SESSION_TTL_MS = 10 * 60_000

export interface PendingAuthorization {
  sessionId: string
  authorizationUrl: string
  expiresAt: number
}

/** One provider instance is permanently bound to one user-owned MCP resource. */
export class PersistentMcpOAuthProvider implements OAuthClientProvider {
  private rawState: string | null = null
  private pendingVerifier: string | null = null
  private authorization: PendingAuthorization | null = null

  constructor(
    private readonly repository: McpRepository,
    private readonly userId: string,
    private readonly serverId: string,
    private readonly resourceUrl: string,
    readonly redirectUrl: string,
    private readonly sessionId?: string,
  ) {}

  get clientMetadata(): OAuthClientMetadata {
    return {
      client_name: 'AI Frontend',
      redirect_uris: [this.redirectUrl],
      grant_types: ['authorization_code'],
      response_types: ['code'],
      token_endpoint_auth_method: 'none',
    }
  }

  async state(): Promise<string> {
    this.rawState = randomBytes(32).toString('base64url')
    return this.rawState
  }

  async clientInformation(): Promise<OAuthClientInformationMixed | undefined> {
    return parseJson<OAuthClientInformationMixed>((await this.credentials())?.clientInformation)
  }

  async saveClientInformation(clientInformation: OAuthClientInformationMixed): Promise<void> {
    await this.saveCredentials({ clientInformation: JSON.stringify(clientInformation) })
  }

  async tokens(): Promise<OAuthTokens | undefined> {
    return parseJson<OAuthTokens>((await this.credentials())?.tokens)
  }

  async saveTokens(tokens: OAuthTokens): Promise<void> {
    await this.saveCredentials({ tokens: JSON.stringify(tokens) })
  }

  async saveDiscoveryState(state: OAuthDiscoveryState): Promise<void> {
    await this.validateResourceURL(this.resourceUrl, state.resourceMetadata?.resource)
    await this.saveCredentials({ discovery: JSON.stringify(state) })
  }

  async discoveryState(): Promise<OAuthDiscoveryState | undefined> {
    return parseJson<OAuthDiscoveryState>((await this.credentials())?.discovery)
  }

  async saveCodeVerifier(codeVerifier: string): Promise<void> {
    this.pendingVerifier = codeVerifier
  }

  async codeVerifier(): Promise<string> {
    if (this.pendingVerifier) return this.pendingVerifier
    const session = await this.loadSession()
    if (!session) throw new Error('OAuth authorization session is unavailable')
    return session.codeVerifier
  }

  async redirectToAuthorization(authorizationUrl: URL): Promise<void> {
    const state = authorizationUrl.searchParams.get('state')
    if (!state || !this.rawState || state !== this.rawState || !this.pendingVerifier) {
      throw new Error('OAuth authorization state could not be persisted safely')
    }
    const now = Date.now()
    const sessionId = uuid()
    const expiresAt = now + OAUTH_SESSION_TTL_MS
    await this.repository.createOAuthSession({
      id: sessionId,
      serverId: this.serverId,
      userId: this.userId,
      stateHash: hashState(state),
      codeVerifier: this.pendingVerifier,
      status: 'pending',
      error: null,
      expiresAt,
      consumedAt: null,
      createdAt: now,
      updatedAt: now,
    })
    this.authorization = { sessionId, authorizationUrl: authorizationUrl.toString(), expiresAt }
    this.rawState = null
    this.pendingVerifier = null
  }

  takePendingAuthorization(): PendingAuthorization | null {
    const pending = this.authorization
    this.authorization = null
    return pending
  }

  async validateResourceURL(serverUrl: string | URL, resource?: string): Promise<URL> {
    const configured = canonicalResource(this.resourceUrl)
    const requestedServer = canonicalResource(serverUrl)
    const discovered = canonicalResource(resource ?? serverUrl)
    if (configured.toString() !== requestedServer.toString() || configured.toString() !== discovered.toString()) {
      throw new Error('OAuth resource does not match the configured MCP server')
    }
    return configured
  }

  async invalidateCredentials(scope: 'all' | 'client' | 'tokens' | 'verifier' | 'discovery'): Promise<void> {
    if (scope === 'all') {
      await this.repository.deleteOAuthCredentials(this.userId, this.serverId)
      await this.cancelVerifier()
      return
    }
    if (scope === 'verifier') {
      await this.cancelVerifier()
      return
    }
    await this.saveCredentials({
      ...(scope === 'client' ? { clientInformation: null } : {}),
      ...(scope === 'tokens' ? { tokens: null } : {}),
      ...(scope === 'discovery' ? { discovery: null } : {}),
    })
  }

  private async credentials() {
    await this.assertBinding()
    const credentials = await this.repository.getOAuthCredentials(this.userId, this.serverId)
    if (credentials && canonicalResource(credentials.resourceUrl).toString() !== canonicalResource(this.resourceUrl).toString()) {
      throw new Error('Stored OAuth credentials belong to a different MCP resource')
    }
    return credentials
  }

  private async saveCredentials(fields: { tokens?: string | null; clientInformation?: string | null; discovery?: string | null }) {
    await this.assertBinding()
    await this.repository.saveOAuthCredentials({
      userId: this.userId,
      serverId: this.serverId,
      resourceUrl: canonicalResource(this.resourceUrl).toString(),
      ...fields,
      updatedAt: Date.now(),
    })
  }

  private async assertBinding(): Promise<void> {
    const server = await this.repository.getServer(this.userId, this.serverId)
    if (!server?.url) throw new Error('MCP server not found')
    if (canonicalResource(server.url).toString() !== canonicalResource(this.resourceUrl).toString()) {
      throw new Error('OAuth provider is bound to a different MCP resource')
    }
  }

  private async loadSession(): Promise<StoredMcpOAuthSession | null> {
    return this.repository.getOAuthSession(this.userId, this.serverId, this.sessionId)
  }

  private async cancelVerifier(): Promise<void> {
    const session = await this.loadSession()
    if (session?.status === 'pending') {
      await this.repository.updateOAuthSession(this.userId, this.serverId, session.id, {
        status: 'cancelled', error: 'Authorization is no longer valid.', updatedAt: Date.now(),
      })
    }
    this.pendingVerifier = null
  }
}

export function hashState(state: string): string {
  return createHash('sha256').update(state).digest('hex')
}

export function canonicalResource(value: string | URL): URL {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.hash || url.search) {
    throw new Error('MCP OAuth resource URL must be an HTTP(S) URL without credentials, query, or fragment')
  }
  url.protocol = url.protocol.toLowerCase()
  url.hostname = url.hostname.toLowerCase()
  if (url.pathname === '/') url.pathname = ''
  return url
}

function parseJson<T>(value: string | null | undefined): T | undefined {
  if (!value) return undefined
  return JSON.parse(value) as T
}
