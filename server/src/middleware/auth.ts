import { createMiddleware } from 'hono/factory'
import { createHash } from 'crypto'
import type { UserRepository } from '../repositories/types.js'

type AuthEnv = {
  Variables: {
    userId: string
    runtime: { repositories: { users: UserRepository } }
  }
}

export const authMiddleware = createMiddleware<AuthEnv>(async (c, next) => {
  // Skip auth for health check
  if (c.req.method === 'GET' && c.req.path === '/health') {
    return next()
  }

  const authHeader = c.req.header('Authorization')
  if (!authHeader?.startsWith('Bearer ')) {
    return c.json({ error: 'Unauthorized' }, 401)
  }

  const token = authHeader.slice(7)
  const hash = createHash('sha256').update(token).digest('hex')

  const runtime = c.get('runtime')
  const user = await runtime.repositories.users.getByApiKeyHash(hash)

  if (!user) {
    return c.json({ error: 'Unauthorized' }, 401)
  }

  c.set('userId', user.id)
  return next()
})
