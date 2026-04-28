import { rateLimiter } from 'hono-rate-limiter'
import type { Context, MiddlewareHandler } from 'hono'
import { getConnInfo } from '@hono/node-server/conninfo'
import { logger } from './logger.js'

function getClientIp(c: Context, trustProxy: boolean): string {
  if (trustProxy) {
    const forwarded = c.req.header('x-forwarded-for')
    if (forwarded) return forwarded.split(',')[0].trim()
    const realIp = c.req.header('x-real-ip')
    if (realIp) return realIp
  }

  return getConnInfo(c).remote.address ?? 'unknown'
}

function getUserId(c: Context): string | undefined {
  const id = c.get('userId' as never)
  return typeof id === 'string' && id.length > 0 ? id : undefined
}

interface LimiterOptions {
  /** Bucket name — surfaces in logs so you can see which limit was hit. */
  name: string
  /** Requests allowed per window. */
  limit: number
  /** Window length in ms (default 60s). */
  windowMs?: number
  /** `user` requires authMiddleware to have run first; falls back to IP. */
  keyBy: 'ip' | 'user'
  /** Trust proxy-provided forwarding headers for client IP extraction. */
  trustProxy?: boolean
  /** Decrement successful requests. Useful for pre-auth failure throttles. */
  skipSuccessfulRequests?: boolean
}

export function createRateLimiter(opts: LimiterOptions): MiddlewareHandler {
  const trustProxy = opts.trustProxy ?? false
  return rateLimiter({
    windowMs: opts.windowMs ?? 60_000,
    limit: opts.limit,
    standardHeaders: 'draft-6',
    skipSuccessfulRequests: opts.skipSuccessfulRequests ?? false,
    keyGenerator: (c) => {
      if (opts.keyBy === 'user') {
        return getUserId(c) ?? `ip:${getClientIp(c, trustProxy)}`
      }
      return getClientIp(c, trustProxy)
    },
    handler: (c) => {
      logger.warn(
        {
          bucket: opts.name,
          ip: getClientIp(c, trustProxy),
          userId: getUserId(c),
          path: c.req.path,
          method: c.req.method,
        },
        'Rate limit exceeded',
      )
      return c.json({ error: 'Too many requests' }, 429)
    },
  })
}
