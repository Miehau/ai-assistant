import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'
import type { ProviderRegistryImpl } from '../providers/registry.js'

export function apiKeyRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // GET /:provider — Check if key exists (never return actual key)
  app.get('/:provider', async (c) => {
    try {
      const { provider } = c.req.param()
      const record = await runtime.repositories.apiKeys.getByProvider(provider)

      return c.json({
        provider,
        exists: record !== null,
        updatedAt: record?.updatedAt ?? null,
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // PUT /:provider — Save API key and register the LLM provider
  app.put('/:provider', async (c) => {
    try {
      const { provider } = c.req.param()
      const body = await c.req.json<{ apiKey: string }>()

      if (!body.apiKey) {
        return c.json({ error: 'apiKey is required' }, 400)
      }

      // Persist to DB
      const record = await runtime.repositories.apiKeys.upsert(
        provider,
        body.apiKey,
      )

      // Hot-register the provider so it's immediately usable
      const registry = runtime.providers as ProviderRegistryImpl
      registry.registerFromKey(provider, body.apiKey)

      return c.json({
        provider: record.provider,
        exists: true,
        registered: runtime.providers.list().includes(provider),
        updatedAt: record.updatedAt,
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // DELETE /:provider — Delete key and unregister the provider
  app.delete('/:provider', async (c) => {
    try {
      const { provider } = c.req.param()

      const existing = await runtime.repositories.apiKeys.getByProvider(provider)
      if (!existing) {
        return c.json({ error: `No API key found for provider: ${provider}` }, 404)
      }

      await runtime.repositories.apiKeys.delete(provider)

      // Remove from active registry
      const registry = runtime.providers as ProviderRegistryImpl
      registry.unregister(provider)

      return c.json({ ok: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // GET / — List all providers and their key status
  app.get('/', async (c) => {
    try {
      const knownProviders = ['anthropic', 'openai', 'ollama', 'openrouter']
      const registered = runtime.providers.list()

      const statuses = await Promise.all(
        knownProviders.map(async (name) => {
          const record = await runtime.repositories.apiKeys.getByProvider(name)
          return {
            provider: name,
            hasKey: record !== null,
            isActive: registered.includes(name),
            updatedAt: record?.updatedAt ?? null,
          }
        }),
      )

      return c.json({ providers: statuses })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  return app
}
