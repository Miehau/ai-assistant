import { invoke } from "@tauri-apps/api/tauri";
import type { OllamaModel } from "$lib/types/ollama";

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

    try {
      const models = await invoke<OllamaModel[]>("discover_ollama_models");
      this.models = models;
      this.available = true;
      return models;
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
