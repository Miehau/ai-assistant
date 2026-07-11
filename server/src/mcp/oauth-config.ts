export function buildOAuthCallbackUrl(
  publicBaseUrl: string | undefined,
  nodeEnv: 'development' | 'production' | 'test',
): string {
  if (!publicBaseUrl) throw new Error('PUBLIC_BASE_URL is required for MCP OAuth')
  const url = new URL(publicBaseUrl)
  if (url.username || url.password || url.search || url.hash || (url.pathname !== '/' && url.pathname !== '')) {
    throw new Error('PUBLIC_BASE_URL must be an origin without credentials, path, query, or fragment')
  }
  const loopback = ['localhost', '127.0.0.1', '::1'].includes(url.hostname)
  if (nodeEnv === 'production' && url.protocol !== 'https:') {
    throw new Error('PUBLIC_BASE_URL must use HTTPS in production')
  }
  if (url.protocol !== 'https:' && !(loopback && url.protocol === 'http:')) {
    throw new Error('PUBLIC_BASE_URL must use HTTPS or loopback HTTP')
  }
  return new URL('/oauth/mcp/callback', url.origin).toString()
}
