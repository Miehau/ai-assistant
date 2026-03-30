import type { ToolHandler, ToolResult, ToolContext } from './types.js'
import { captureResponseCookies, getCookieHeader, clearSessionCookies, sessionCookieCount } from './cookie-store.js'

const MAX_RESPONSE_BYTES = 200 * 1024 // 200KB

export function registerWebTools(registry: { register: (h: ToolHandler) => void }): void {
  registry.register({
    metadata: {
      name: 'web.fetch',
      description: 'HTTP GET request. Returns raw text or JSON response (max 200KB).',
      parameters: {
        type: 'object',
        properties: {
          url: { type: 'string', description: 'URL to fetch' },
          headers: { type: 'object', description: 'Optional HTTP headers' },
        },
        required: ['url'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const url = args.url as string
      const headers = (args.headers as Record<string, string>) ?? {}

      try {
        const response = await fetchWithCookies(ctx.session_id, url, {
          method: 'GET',
          headers,
        }, ctx.signal)

        const body = await readResponseBody(response)

        return {
          ok: true,
          output: {
            status: response.status,
            headers: Object.fromEntries(response.headers.entries()),
            body,
          },
        }
      } catch (err) {
        if (ctx.signal.aborted) {
          return { ok: false, error: 'Request aborted' }
        }
        return { ok: false, error: `Fetch failed: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `GET ${args.url}` }
    },
  })

  // ── web.cookies.clear ───────────────────────────────────────────────────
  registry.register({
    metadata: {
      name: 'web.cookies.clear',
      description: 'Clear all stored cookies for the current session.',
      parameters: { type: 'object', properties: {} },
      requires_approval: false,
    },
    async handle(_args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const count = sessionCookieCount(ctx.session_id)
      clearSessionCookies(ctx.session_id)
      return { ok: true, output: { cleared: count } }
    },
  })

  registry.register({
    metadata: {
      name: 'web.request',
      description: 'HTTP request with any method. Cookies are automatically saved from responses and sent on subsequent requests (session-scoped).',
      parameters: {
        type: 'object',
        properties: {
          method: { type: 'string', description: 'HTTP method: GET, POST, PUT, DELETE, or PATCH' },
          url: { type: 'string', description: 'URL to request' },
          headers: {
            type: 'object',
            description: 'HTTP headers as key-value pairs',
            additionalProperties: { type: 'string' },
          },
          body: { type: 'string', description: 'Raw request body string (e.g. JSON payload)' },
        },
        required: ['method', 'url'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const method = (args.method as string).toUpperCase()
      const url = args.url as string
      const headers = (args.headers as Record<string, string>) ?? {}
      const body = args.body as string | undefined

      try {
        const response = await fetchWithCookies(ctx.session_id, url, {
          method,
          headers,
          body: body ?? undefined,
        }, ctx.signal)

        const responseBody = await readResponseBody(response)

        return {
          ok: true,
          output: {
            status: response.status,
            headers: Object.fromEntries(response.headers.entries()),
            body: responseBody,
          },
        }
      } catch (err) {
        if (ctx.signal.aborted) {
          return { ok: false, error: 'Request aborted' }
        }
        return { ok: false, error: `Request failed: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `${(args.method as string).toUpperCase()} ${args.url}` }
    },
  })

  // ── web.post_form ───────────────────────────────────────────────────────
  registry.register({
    metadata: {
      name: 'web.post_form',
      description: 'POST form data (application/x-www-form-urlencoded). Use for login forms, HTML form submissions, and APIs expecting form-encoded bodies. Cookies are automatically saved and sent.',
      parameters: {
        type: 'object',
        properties: {
          url: { type: 'string', description: 'URL to POST to' },
          fields: {
            type: 'object',
            description: 'Form fields as key-value string pairs, e.g. {"username":"john","password":"secret"}',
            additionalProperties: { type: 'string' },
          },
          headers: {
            type: 'object',
            description: 'Extra HTTP headers (Content-Type is set automatically)',
            additionalProperties: { type: 'string' },
          },
        },
        required: ['url', 'fields'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const url = args.url as string
      const fields = args.fields as Record<string, string>
      const headers = (args.headers as Record<string, string>) ?? {}

      const body = new URLSearchParams(fields).toString()
      headers['content-type'] ??= 'application/x-www-form-urlencoded'

      try {
        const response = await fetchWithCookies(ctx.session_id, url, {
          method: 'POST',
          headers,
          body,
        }, ctx.signal)

        const responseBody = await readResponseBody(response)

        return {
          ok: true,
          output: {
            status: response.status,
            headers: Object.fromEntries(response.headers.entries()),
            body: responseBody,
          },
        }
      } catch (err) {
        if (ctx.signal.aborted) {
          return { ok: false, error: 'Request aborted' }
        }
        return { ok: false, error: `POST form failed: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `POST form ${args.url}` }
    },
  })
}

const MAX_REDIRECTS = 10

/**
 * Fetch with manual redirect handling so we can capture Set-Cookie headers
 * from every intermediate response (302s, 303s, 307s, etc.).
 */
async function fetchWithCookies(
  sessionId: string,
  url: string,
  init: RequestInit,
  signal?: AbortSignal,
): Promise<Response> {
  let currentUrl = url
  let currentInit = { ...init, redirect: 'manual' as const, signal }

  // Inject cookies for the initial request
  const initHeaders: Record<string, string> = (currentInit.headers as Record<string, string>) ?? {}
  injectCookies(sessionId, currentUrl, initHeaders)
  currentInit.headers = initHeaders

  for (let i = 0; i < MAX_REDIRECTS; i++) {
    const response = await fetch(currentUrl, currentInit)

    // Capture cookies from every response, including redirects
    captureResponseCookies(sessionId, currentUrl, response)

    const status = response.status
    if (status < 300 || status >= 400 || !response.headers.get('location')) {
      return response
    }

    // Resolve redirect URL (may be relative)
    const location = response.headers.get('location')!
    currentUrl = new URL(location, currentUrl).href

    // 302/303 → switch to GET with no body; 307/308 → preserve method & body
    if (status === 302 || status === 303 || status === 301) {
      currentInit = { redirect: 'manual', signal, headers: {} }
    }

    // Inject cookies for the next hop
    const nextHeaders: Record<string, string> = {}
    injectCookies(sessionId, currentUrl, nextHeaders)
    currentInit.headers = nextHeaders
  }

  throw new Error(`Too many redirects (>${MAX_REDIRECTS})`)
}

async function readResponseBody(response: Response): Promise<unknown> {
  const text = await response.text()

  if (Buffer.byteLength(text) > MAX_RESPONSE_BYTES) {
    return text.slice(0, MAX_RESPONSE_BYTES) + '\n... [truncated]'
  }

  // Try to parse as JSON
  const contentType = response.headers.get('content-type') ?? ''
  if (contentType.includes('json')) {
    try {
      return JSON.parse(text)
    } catch {
      // Return as text if JSON parsing fails
    }
  }

  return text
}

/** Attach stored cookies to request headers (does not overwrite manually-set Cookie header). */
function injectCookies(sessionId: string, url: string, headers: Record<string, string>): void {
  // Don't overwrite an explicit Cookie header from the caller
  if (headers['cookie'] || headers['Cookie']) return

  const cookie = getCookieHeader(sessionId, url)
  if (cookie) headers['cookie'] = cookie
}
