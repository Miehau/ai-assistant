import type { LLMProvider, ProviderRegistry } from './types.js'
import type { ApiKeyRepository } from '../repositories/types.js'
import { AnthropicProvider } from './anthropic.js'
import { OpenAIProvider } from './openai.js'
import { OllamaProvider } from './ollama.js'
import { OpenRouterProvider } from './openrouter.js'
import { logger } from '../lib/logger.js'

/**
 * Map of known provider names → factory functions.
 * Each factory takes an API key (or base URL for Ollama) and returns an LLMProvider.
 */
const PROVIDER_FACTORIES: Record<string, (key: string) => LLMProvider> = {
  anthropic: (key) => new AnthropicProvider(key),
  openai: (key) => new OpenAIProvider(key),
  ollama: (baseUrl) => new OllamaProvider(baseUrl),
  openrouter: (key) => new OpenRouterProvider(key),
}

export class ProviderRegistryImpl implements ProviderRegistry {
  private providers = new Map<string, LLMProvider>()
  private apiKeys: ApiKeyRepository | null = null

  /**
   * Attach an ApiKeyRepository so the registry can load DB-stored keys at startup.
   */
  setApiKeyRepository(repo: ApiKeyRepository): void {
    this.apiKeys = repo
  }

  register(name: string, provider: LLMProvider): void {
    this.providers.set(name, provider)
  }

  /**
   * Create and register a provider from a name + API key using the built-in factory.
   * Returns true if the provider was created, false if the name is unknown.
   */
  registerFromKey(name: string, apiKey: string): boolean {
    const factory = PROVIDER_FACTORIES[name]
    if (!factory) {
      logger.warn({ provider: name }, 'Unknown provider name — cannot auto-create from key')
      return false
    }
    this.providers.set(name, factory(apiKey))
    logger.info({ provider: name }, 'Provider registered from API key')
    return true
  }

  /**
   * Remove a provider from the registry.
   */
  unregister(name: string): void {
    this.providers.delete(name)
    logger.info({ provider: name }, 'Provider unregistered')
  }

  /**
   * Resolve a provider from a model string in "provider:model" format.
   * Throws if the provider is not registered or the format is invalid.
   */
  resolve(modelString: string): LLMProvider {
    const separatorIndex = modelString.indexOf(':')
    if (separatorIndex === -1) {
      throw new Error(
        `Invalid model format "${modelString}". Expected "provider:model" (e.g. "openai:gpt-4o"). Registered providers: ${this.list().join(', ') || '(none)'}`,
      )
    }

    const providerName = modelString.slice(0, separatorIndex)
    const provider = this.providers.get(providerName)
    if (!provider) {
      throw new Error(
        `Provider "${providerName}" not found. Registered providers: ${this.list().join(', ') || '(none)'}. Save an API key via PUT /api/keys/${providerName} to enable it.`,
      )
    }

    return provider
  }

  list(): string[] {
    return Array.from(this.providers.keys())
  }

  /**
   * Load API keys from the database and register any providers that aren't
   * already registered (env vars take precedence).
   */
  async loadKeysFromDatabase(): Promise<string[]> {
    if (!this.apiKeys) return []

    const loaded: string[] = []
    for (const name of Object.keys(PROVIDER_FACTORIES)) {
      // Skip if already registered (e.g. from .env)
      if (this.providers.has(name)) continue

      const record = await this.apiKeys.getByProvider(name)
      if (record) {
        this.registerFromKey(name, record.encryptedKey)
        loaded.push(name)
      }
    }

    if (loaded.length > 0) {
      logger.info({ providers: loaded }, 'Providers loaded from database')
    }
    return loaded
  }
}
