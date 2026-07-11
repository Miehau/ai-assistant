import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'
import { hashState } from '../mcp/oauth-provider.js'

const CALLBACK_CSP = "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"

export function mcpOAuthCallbackRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  app.get('/callback', async (c) => {
    const state = c.req.query('state')
    const code = c.req.query('code')
    const providerError = c.req.query('error')
    if (!state || state.length > 512 || Boolean(code) === Boolean(providerError) || (code?.length ?? 0) > 4096 || (providerError?.length ?? 0) > 128) {
      return callbackHtml(c, false, 'The authorization response is invalid.', 400)
    }

    const stateHash = hashState(state)
    const now = Date.now()
    const session = await runtime.repositories.mcp.consumeOAuthSession(stateHash, now)
    if (!session) {
      const known = await runtime.repositories.mcp.getOAuthSessionByStateHash(stateHash)
      if (known?.status === 'pending' && known.expiresAt <= now) {
        await runtime.repositories.mcp.updateOAuthSession(known.userId, known.serverId, known.id, {
          status: 'expired', error: 'Authorization expired. Start again.', updatedAt: now,
        })
      }
      return callbackHtml(c, false, 'This authorization request is no longer valid. Return to the app and try again.', 410)
    }

    if (providerError) {
      const denied = providerError === 'access_denied'
      await runtime.repositories.mcp.updateOAuthSession(session.userId, session.serverId, session.id, {
        status: denied ? 'denied' : 'error',
        error: denied ? 'Authorization was denied.' : 'Authorization failed at the provider.',
        updatedAt: now,
      })
      return callbackHtml(c, false, denied ? 'Authorization was denied. You can close this window.' : 'Authorization failed. Return to the app and try again.', 400)
    }

    try {
      await runtime.mcps.finishOAuth(session.userId, session.serverId, session.id, code!)
      return callbackHtml(c, true, 'Authorization complete. You can close this window and return to the app.', 200)
    } catch {
      return callbackHtml(c, false, 'Authorization could not be completed. Return to the app and try again.', 400)
    }
  })

  return app
}

function callbackHtml(c: any, success: boolean, message: string, status: 200 | 400 | 410) {
  c.header('Cache-Control', 'no-store, max-age=0')
  c.header('Pragma', 'no-cache')
  c.header('Content-Security-Policy', CALLBACK_CSP)
  c.header('Referrer-Policy', 'no-referrer')
  c.header('X-Content-Type-Options', 'nosniff')
  c.header('X-Frame-Options', 'DENY')
  return c.html(`<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>MCP authorization</title><style>body{font:16px system-ui;margin:3rem;max-width:40rem}h1{font-size:1.5rem}</style></head><body><main><h1>${success ? 'Connected' : 'Authorization not completed'}</h1><p>${message}</p></main></body></html>`, status)
}
