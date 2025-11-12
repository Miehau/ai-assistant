import { invoke } from "@tauri-apps/api/tauri";
import { modelRegistry } from "./registry";

/**
 * Service for managing API keys for different providers
 */
export class ApiKeyService {
  private apiKeys: Record<string, string> = {};

  /**
   * Load all API keys from storage
   */
  public async loadAllApiKeys(): Promise<Record<string, string>> {
    const providers = modelRegistry.getAllProviders();
    this.apiKeys = {};

    console.log('Loading API keys for providers:', providers.map(p => p.id));

    for (const provider of providers) {
      try {
        const key = await invoke<string | null>("get_api_key", { provider: provider.id });
        if (key) {
          this.apiKeys[provider.id] = key;
          console.log(`Loaded API key for ${provider.id}`);
        } else {
          console.log(`No API key found for ${provider.id}`);
        }
      } catch (error) {
        console.error(`Error loading API key for ${provider.name}:`, error);
      }
    }

    console.log('Loaded API keys for providers:', Object.keys(this.apiKeys));

    // Update available models based on API keys
    modelRegistry.updateAvailableModels(this.apiKeys);
    
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
      await invoke<string>("set_api_key", { provider: providerId, api_key: apiKey });
      this.apiKeys[providerId] = apiKey;
      
      // Update available models after setting a new API key
      modelRegistry.updateAvailableModels(this.apiKeys);
      
      return true;
    } catch (error) {
      console.error(`Error setting API key for ${providerId}:`, error);
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
      
      // Update available models after deleting an API key
      modelRegistry.updateAvailableModels(this.apiKeys);
      
      return true;
    } catch (error) {
      console.error(`Error deleting API key for ${providerId}:`, error);
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
}
