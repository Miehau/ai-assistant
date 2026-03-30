/**
 * Simple in-memory cookie store, scoped per session.
 *
 * Captures Set-Cookie headers from responses and attaches matching cookies
 * to subsequent requests — mimicking a browser cookie jar.
 */

interface Cookie {
  name: string
  value: string
  domain: string
  path: string
  expires?: number // epoch ms, undefined = session cookie
  secure: boolean
  httpOnly: boolean
  sameSite: 'strict' | 'lax' | 'none'
}

/** session_id → cookies[] */
const sessions = new Map<string, Cookie[]>()

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** Parse Set-Cookie headers from a Response and store them for this session. */
export function captureResponseCookies(
  sessionId: string,
  requestUrl: string,
  response: Response,
): void {
  const setCookieHeaders = response.headers.getSetCookie?.() ?? []
  if (setCookieHeaders.length === 0) return

  const url = new URL(requestUrl)

  for (const header of setCookieHeaders) {
    const cookie = parseSetCookie(header, url)
    if (cookie) upsert(sessionId, cookie)
  }
}

/** Build a Cookie header value for the given URL using stored cookies. */
export function getCookieHeader(sessionId: string, requestUrl: string): string | undefined {
  const jar = sessions.get(sessionId)
  if (!jar || jar.length === 0) return undefined

  const url = new URL(requestUrl)
  const now = Date.now()

  const matching = jar.filter((c) => {
    // Expired?
    if (c.expires !== undefined && c.expires <= now) return false
    // Domain match (suffix match)
    if (!domainMatches(url.hostname, c.domain)) return false
    // Path match (prefix match)
    if (!url.pathname.startsWith(c.path)) return false
    // Secure?
    if (c.secure && url.protocol !== 'https:') return false
    return true
  })

  if (matching.length === 0) return undefined

  // Sort: longer paths first (more specific)
  matching.sort((a, b) => b.path.length - a.path.length)

  return matching.map((c) => `${c.name}=${c.value}`).join('; ')
}

/** Clear all cookies for a session. */
export function clearSessionCookies(sessionId: string): void {
  sessions.delete(sessionId)
}

/** Number of cookies stored for a session (for diagnostics). */
export function sessionCookieCount(sessionId: string): number {
  return sessions.get(sessionId)?.length ?? 0
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

function parseSetCookie(header: string, requestUrl: URL): Cookie | null {
  const parts = header.split(';').map((s) => s.trim())
  const [first, ...attrs] = parts
  if (!first) return null

  const eqIdx = first.indexOf('=')
  if (eqIdx < 1) return null

  const name = first.slice(0, eqIdx).trim()
  const value = first.slice(eqIdx + 1).trim()

  const cookie: Cookie = {
    name,
    value,
    domain: requestUrl.hostname,
    path: '/',
    secure: false,
    httpOnly: false,
    sameSite: 'lax',
  }

  for (const attr of attrs) {
    const [rawKey, ...rawValParts] = attr.split('=')
    const key = rawKey!.trim().toLowerCase()
    const val = rawValParts.join('=').trim()

    switch (key) {
      case 'domain':
        cookie.domain = val.startsWith('.') ? val.slice(1) : val
        break
      case 'path':
        cookie.path = val || '/'
        break
      case 'expires': {
        const d = new Date(val)
        if (!isNaN(d.getTime())) cookie.expires = d.getTime()
        break
      }
      case 'max-age': {
        const seconds = parseInt(val, 10)
        if (!isNaN(seconds)) {
          cookie.expires = seconds <= 0 ? 0 : Date.now() + seconds * 1000
        }
        break
      }
      case 'secure':
        cookie.secure = true
        break
      case 'httponly':
        cookie.httpOnly = true
        break
      case 'samesite':
        cookie.sameSite = val.toLowerCase() as Cookie['sameSite']
        break
    }
  }

  return cookie
}

function upsert(sessionId: string, cookie: Cookie): void {
  let jar = sessions.get(sessionId)
  if (!jar) {
    jar = []
    sessions.set(sessionId, jar)
  }

  // Remove expired cookie (Max-Age=0 or Expires in the past)
  if (cookie.expires !== undefined && cookie.expires <= Date.now()) {
    const idx = jar.findIndex(
      (c) => c.name === cookie.name && c.domain === cookie.domain && c.path === cookie.path,
    )
    if (idx !== -1) jar.splice(idx, 1)
    return
  }

  // Replace existing or append
  const idx = jar.findIndex(
    (c) => c.name === cookie.name && c.domain === cookie.domain && c.path === cookie.path,
  )
  if (idx !== -1) {
    jar[idx] = cookie
  } else {
    jar.push(cookie)
  }
}

function domainMatches(hostname: string, cookieDomain: string): boolean {
  if (hostname === cookieDomain) return true
  // Suffix match: hostname ends with ".cookieDomain"
  return hostname.endsWith('.' + cookieDomain)
}
