import { invoke } from "@tauri-apps/api/tauri";
import { modelRegistry } from "./registry";
import type { Model } from "$lib/types/models";
import type { ModelConfig } from "./registry/types";

/**
 * Service for managing models and converting between registry models and app models
 */
export class ModelService {
  /**
   * Load all models from storage
   */
  public async loadModels(): Promise<Model[]> {
    try {
      const storedModels = await invoke<Model[]>("get_models");
      return storedModels;
    } catch (error) {
      console.error("Failed to load models:", error);
      return [];
    }
  }

  /**
   * Add a new model
   */
  public async addModel(model: Omit<Model, "id">): Promise<boolean> {
    try {
      await invoke<string>("add_model", { model });
      return true;
    } catch (error) {
      console.error("Failed to add model:", error);
      return false;
    }
  }

  /**
   * Toggle model enabled status
   */
  public async toggleModel(model: Pick<Model, "provider" | "model_name">): Promise<boolean> {
    try {
      await invoke("toggle_model", { model });
      return true;
    } catch (error) {
      console.error("Failed to toggle model:", error);
      return false;
    }
  }

  /**
   * Delete a model
   */
  public async deleteModel(model: Model): Promise<boolean> {
    try {
      await invoke("delete_model", { model });
      return true;
    } catch (error) {
      console.error("Failed to delete model:", error);
      return false;
    }
  }

  /**
   * Get available models from registry with capabilities
   */
  public getAvailableModelsWithCapabilities(): Model[] {
    const registryModels = modelRegistry.getAllModels();
    
    // Convert registry models to app models
    return Object.entries(registryModels).map(([modelId, config]) => ({
      model_name: modelId, // Use the model ID (key) as the model_name for API calls
      name: config.name,   // Use the human-readable name for display
      provider: config.provider,
      enabled: true,
      // Add capabilities as properties
      capabilities: config.capabilities,
      specs: config.specs
    }));
  }

  /**
   * Get models by capability
   */
  public getModelsByCapability(capability: keyof ModelConfig['capabilities']): Model[] {
    const registryModels = modelRegistry.getModelsByCapability(capability);
    
    return Object.entries(registryModels).map(([modelName, config]) => ({
      model_name: modelName,
      provider: config.provider,
      enabled: true,
      capabilities: config.capabilities,
      specs: config.specs
    }));
  }

  /**
   * Get models for a specific provider
   */
  public getModelsByProvider(providerId: string): Model[] {
    const registryModels = modelRegistry.getModelsByProvider(providerId);
    
    return Object.entries(registryModels).map(([modelName, config]) => ({
      model_name: modelName,
      provider: config.provider,
      enabled: true,
      capabilities: config.capabilities,
      specs: config.specs
    }));
  }
}

// Export an instance of the ModelService
export const modelService = new ModelService();
