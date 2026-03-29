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
    // Ollama discovery not yet implemented in server backend
    console.warn('[ollamaService] discoverModels not yet implemented in server backend');
    this.available = false;
    this.models = [];
    return [];
  }
}

export const ollamaService = new OllamaService();
