import { modelRegistry } from "./registry";

/**
 * Service for managing API keys for different providers
 * Uses Svelte 5 runes for reactivity
 */
export class ApiKeyService {
  apiKeys = $state<Record<string, string>>({});
  providerAvailability = $state<Record<string, boolean>>({});

  /**
   * Load all API keys from storage
   */
  public async loadAllApiKeys(): Promise<Record<string, string>> {
    // Not yet implemented in server backend
    console.warn('[ApiKeyService] loadAllApiKeys not yet implemented in server backend');
    this.apiKeys = {};
    this.providerAvailability = {};
    modelRegistry.updateAvailableModels(this.apiKeys, this.providerAvailability);
    return this.apiKeys;
  }

  /**
   * Get API key for a specific provider
   */
  public async getApiKey(providerId: string): Promise<string | null> {
    console.warn('[ApiKeyService] getApiKey not yet implemented in server backend');
    return null;
  }

  /**
   * Set API key for a specific provider
   */
  public async setApiKey(providerId: string, apiKey: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  /**
   * Delete API key for a specific provider
   */
  public async deleteApiKey(providerId: string): Promise<boolean> {
    throw new Error('Not yet implemented in server backend');
  }

  /**
   * Get all loaded API keys
   */
  public getAllApiKeys(): Record<string, string> {
    return { ...this.apiKeys };
  }

  /**
   * Check if a provider has an API key
   */
  public hasApiKey(providerId: string): boolean {
    return !!this.apiKeys[providerId];
  }

  private async refreshProviderAvailability(providers = modelRegistry.getAllProviders()): Promise<void> {
    // Not yet implemented in server backend
    console.warn('[ApiKeyService] refreshProviderAvailability not yet implemented in server backend');
    const availability: Record<string, boolean> = {};
    for (const provider of providers) {
      availability[provider.id] = true;
    }
    this.providerAvailability = availability;
  }
}
