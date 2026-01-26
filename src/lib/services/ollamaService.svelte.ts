import { invoke } from "@tauri-apps/api/tauri";
import type { OllamaDiscoveryResult, OllamaModel } from "$lib/types/ollama";
import { modelRegistry } from "$lib/models/registry";

/**
 * Service for managing Ollama discovery
 * Uses Svelte 5 runes for reactivity
 */
export class OllamaService {
  models = $state<OllamaModel[]>([]);
  available = $state<boolean>(false);
  loading = $state<boolean>(false);
  error = $state<string | null>(null);

  /**
   * Discover available Ollama models.
   * Non-blocking usage should fire-and-forget this method.
   */
  async discoverModels(): Promise<OllamaModel[]> {
    this.loading = true;
    this.error = null;
    const baseUrl = modelRegistry.getProviderUrl('ollama') || 'http://localhost:11434/v1';

    try {
      const result = await invoke<OllamaDiscoveryResult>("discover_ollama_models", {
        base_url: baseUrl,
      });
      this.models = result.models;
      this.available = result.available;
      return result.models;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.error = message;
      this.available = false;
      this.models = [];
      return [];
    } finally {
      this.loading = false;
    }
  }
}

export const ollamaService = new OllamaService();
