import type { ToolHandler, ToolResult, ToolContext } from './types.js'

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
        const response = await fetch(url, {
          method: 'GET',
          headers,
          signal: ctx.signal,
        })

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

  registry.register({
    metadata: {
      name: 'web.request',
      description: 'Generic HTTP request. Supports all methods, custom headers, and request body.',
      parameters: {
        type: 'object',
        properties: {
          method: { type: 'string', description: 'HTTP method (GET, POST, PUT, DELETE, PATCH)' },
          url: { type: 'string', description: 'URL to request' },
          headers: { type: 'object', description: 'HTTP headers' },
          body: { type: 'string', description: 'Request body (string)' },
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
        const response = await fetch(url, {
          method,
          headers,
          body: body ?? undefined,
          signal: ctx.signal,
        })

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
