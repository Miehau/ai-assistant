import { invoke } from "@tauri-apps/api/tauri";
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
    const providers = modelRegistry.getAllProviders();
    const loadedKeys: Record<string, string> = {};

    console.log('[ApiKeyService] Loading API keys for providers:', providers.map(p => p.id));

    for (const provider of providers) {
      if (provider.authType !== 'api_key') {
        continue;
      }
      try {
        const key = await invoke<string | null>("get_api_key", { provider: provider.id });
        if (key) {
          loadedKeys[provider.id] = key;
          console.log(`[ApiKeyService] Loaded API key for ${provider.id}`);
        } else {
          console.log(`[ApiKeyService] No API key found for ${provider.id}`);
        }
      } catch (error) {
        console.error(`[ApiKeyService] Error loading API key for ${provider.name}:`, error);
      }
    }

    // Assign all keys at once to prevent race conditions
    this.apiKeys = loadedKeys;

    console.log('[ApiKeyService] Loaded API keys for providers:', Object.keys(this.apiKeys));
    console.log('[ApiKeyService] About to update model registry with keys:', Object.keys(this.apiKeys));

    // Update provider availability for non-key providers
    await this.refreshProviderAvailability(providers);

    // Update available models based on API keys + provider availability
    modelRegistry.updateAvailableModels(this.apiKeys, this.providerAvailability);

    return this.apiKeys;
  }

  /**
   * Get API key for a specific provider
   */
  public async getApiKey(providerId: string): Promise<string | null> {
    try {
      const key = await invoke<string | null>("get_api_key", { provider: providerId });
      if (key) {
        this.apiKeys[providerId] = key;
        return key;
      }
    } catch (error) {
      console.error(`Error loading API key for ${providerId}:`, error);
    }
    return null;
  }

  /**
   * Set API key for a specific provider
   */
  public async setApiKey(providerId: string, apiKey: string): Promise<boolean> {
    try {
      await invoke<string>("set_api_key", { provider: providerId, apiKey: apiKey });
      this.apiKeys[providerId] = apiKey;

      console.log(`[ApiKeyService] Set API key for ${providerId}, current keys:`, Object.keys(this.apiKeys));

      // Update available models after setting a new API key
      modelRegistry.updateAvailableModels(this.apiKeys, this.providerAvailability);

      return true;
    } catch (error) {
      console.error(`[ApiKeyService] Error setting API key for ${providerId}:`, error);
      return false;
    }
  }

  /**
   * Delete API key for a specific provider
   */
  public async deleteApiKey(providerId: string): Promise<boolean> {
    try {
      await invoke<void>("delete_api_key", { provider: providerId });
      delete this.apiKeys[providerId];

      console.log(`[ApiKeyService] Deleted API key for ${providerId}, remaining keys:`, Object.keys(this.apiKeys));

      // Update available models after deleting an API key
      modelRegistry.updateAvailableModels(this.apiKeys, this.providerAvailability);

      return true;
    } catch (error) {
      console.error(`[ApiKeyService] Error deleting API key for ${providerId}:`, error);
      return false;
    }
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
    const availability: Record<string, boolean> = {};

    for (const provider of providers) {
      if (provider.authType === 'none' && provider.id === 'claude_cli') {
        try {
          availability[provider.id] = await invoke<boolean>('is_claude_cli_installed');
        } catch (error) {
          console.error(`[ApiKeyService] Error checking Claude CLI availability:`, error);
          availability[provider.id] = false;
        }
        continue;
      }

      availability[provider.id] = true;
    }

    this.providerAvailability = availability;
  }
}
