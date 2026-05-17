import type { ToolHandler, ToolResult, ToolContext } from './types.js'
import { captureResponseCookies, getCookieHeader, clearSessionCookies, sessionCookieCount } from './cookie-store.js'

const MAX_RESPONSE_BYTES = 200 * 1024 // 200KB

export function registerWebTools(registry: { register: (h: ToolHandler) => void }): void {
  registry.register({
    metadata: {
      name: 'web.fetch',
      description: 'HTTP GET request.',
      parameters: {
        type: 'object',
        properties: {
          url: { type: 'string', description: 'URL to fetch' },
          headers: { type: 'object', description: 'Headers' },
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

        const body = await readResponseBody(response, { extractHtmlBody: true })

        return {
          ok: true,
          output: response.ok ? body : { status: response.status, body },
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
      description: 'Clear session cookies.',
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
      description: 'HTTP request with session cookies.',
      parameters: {
        type: 'object',
        properties: {
          method: { type: 'string', description: 'GET, POST, PUT, DELETE, or PATCH' },
          url: { type: 'string', description: 'URL' },
          headers: {
            type: 'object',
            description: 'Headers',
            additionalProperties: { type: 'string' },
          },
          body: { type: 'string', description: 'Raw body' },
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

        const responseBody = await readResponseBody(response, { extractHtmlBody: false })

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
      description: 'POST form data with session cookies.',
      parameters: {
        type: 'object',
        properties: {
          url: { type: 'string', description: 'URL' },
          fields: {
            type: 'object',
            description: 'Form fields',
            additionalProperties: { type: 'string' },
          },
          headers: {
            type: 'object',
            description: 'Headers',
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

        const responseBody = await readResponseBody(response, { extractHtmlBody: true })

        return {
          ok: true,
          output: {
            status: response.status,
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

async function readResponseBody(
  response: Response,
  options: { extractHtmlBody: boolean },
): Promise<unknown> {
  const text = await response.text()

  const contentType = response.headers.get('content-type') ?? ''
  if (contentType.includes('json')) {
    if (Buffer.byteLength(text) > MAX_RESPONSE_BYTES) {
      return text.slice(0, MAX_RESPONSE_BYTES) + '\n... [truncated]'
    }
    try {
      return JSON.parse(text)
    } catch {
      // Return as text if JSON parsing fails
    }
  }

  const body = options.extractHtmlBody && contentType.includes('html')
    ? extractReadableHtmlBody(text)
    : text

  if (Buffer.byteLength(body) > MAX_RESPONSE_BYTES) {
    return body.slice(0, MAX_RESPONSE_BYTES) + '\n... [truncated]'
  }

  return body
}

function extractReadableHtmlBody(html: string): string {
  const body = extractHtmlBodyFragment(html)
  return decodeHtmlEntities(
    body
      .replace(/<(script|style|template|noscript|svg)\b[\s\S]*?<\/\1>/gi, ' ')
      .replace(/<!--[\s\S]*?-->/g, ' ')
      .replace(/<(br|hr)\b[^>]*>/gi, '\n')
      .replace(/<\/(p|div|section|article|header|footer|main|aside|nav|li|tr|h[1-6])>/gi, '\n')
      .replace(/<[^>]+>/g, ' ')
      .replace(/\r\n?/g, '\n')
      .replace(/[ \t\f\v]+/g, ' ')
      .replace(/ *\n */g, '\n')
      .replace(/\n{3,}/g, '\n\n')
      .trim(),
  )
}

function extractHtmlBodyFragment(html: string): string {
  const bodyMatch = html.match(/<body\b[^>]*>([\s\S]*?)<\/body>/i)
  if (bodyMatch) return bodyMatch[1]

  const openBodyMatch = /<body\b[^>]*>/i.exec(html)
  if (openBodyMatch) return html.slice(openBodyMatch.index + openBodyMatch[0].length)

  return html.replace(/<head\b[\s\S]*?<\/head>/i, ' ')
}

function decodeHtmlEntities(text: string): string {
  return text
    .replace(/&#(\d+);/g, (match, code: string) => decodeCodePoint(match, Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (match, code: string) => decodeCodePoint(match, Number.parseInt(code, 16)))
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/g, "'")
}

function decodeCodePoint(fallback: string, codePoint: number): string {
  try {
    return Number.isFinite(codePoint) ? String.fromCodePoint(codePoint) : fallback
  } catch {
    return fallback
  }
}

/** Attach stored cookies to request headers (does not overwrite manually-set Cookie header). */
function injectCookies(sessionId: string, url: string, headers: Record<string, string>): void {
  // Don't overwrite an explicit Cookie header from the caller
  if (headers['cookie'] || headers['Cookie']) return

  const cookie = getCookieHeader(sessionId, url)
  if (cookie) headers['cookie'] = cookie
}
