import type { ModelConfig, ModelsConfig, ProviderConfig, ProvidersConfig } from './types';
import providersConfig from './providers.json';
import modelsConfig from './models.json';

export class ModelRegistryService {
  private providers: ProviderConfig[];
  private models: Record<string, ModelConfig> = {};
  private availableModels: Record<string, ModelConfig> = {};

  constructor() {
    this.providers = (providersConfig as ProvidersConfig).providers;
    
    // Convert array of models to a record for easier lookup
    const modelArray = (modelsConfig as ModelsConfig).models;
    modelArray.forEach(model => {
      this.models[model.id] = model;
    });
    
    this.updateAvailableModels();
    console.log('ModelRegistryService initialized with models:', Object.keys(this.models).length);
  }

  /**
   * Updates the list of available models based on API key availability
   */
  public updateAvailableModels(apiKeys: Record<string, string> = {}): void {
    this.availableModels = {};
    
    console.log('Updating available models with API keys:', Object.keys(apiKeys));
    
    // For development/testing, make all models available regardless of API keys
    // This ensures models show up in the UI even without API keys
    Object.entries(this.models).forEach(([modelName, modelConfig]) => {
      this.availableModels[modelName] = modelConfig;
      console.log(`Added model ${modelName} from provider ${modelConfig.provider}`);
    });
    
    console.log('Available models after update:', Object.keys(this.availableModels).length);
  }

  /**
   * Get all available models
   */
  public getAllModels(): Record<string, ModelConfig> {
    return this.availableModels;
  }

  /**
   * Get models filtered by capability
   */
  public getModelsByCapability(capability: keyof ModelConfig['capabilities']): Record<string, ModelConfig> {
    return Object.entries(this.availableModels)
      .filter(([_, model]) => model.capabilities[capability])
      .reduce((acc, [name, model]) => {
        acc[name] = model;
        return acc;
      }, {} as Record<string, ModelConfig>);
  }

  /**
   * Get models for a specific provider
   */
  public getModelsByProvider(providerId: string): Record<string, ModelConfig> {
    return Object.entries(this.availableModels)
      .filter(([_, model]) => model.provider === providerId)
      .reduce((acc, [name, model]) => {
        acc[name] = model;
        return acc;
      }, {} as Record<string, ModelConfig>);
  }

  /**
   * Get a specific model by name
   */
  public getModel(modelName: string): ModelConfig | undefined {
    return this.availableModels[modelName];
  }

  /**
   * Get all providers
   */
  public getAllProviders(): ProviderConfig[] {
    return this.providers;
  }

  /**
   * Get a specific provider by ID
   */
  public getProvider(providerId: string): ProviderConfig | undefined {
    return this.providers.find(provider => provider.id === providerId);
  }

  /**
   * Validate if a model is available for use
   */
  public isModelAvailable(modelName: string): boolean {
    return !!this.availableModels[modelName];
  }

  /**
   * Get provider-specific headers with API key
   */
  public getProviderHeaders(providerId: string, apiKey: string): Record<string, string> {
    const provider = this.getProvider(providerId);
    if (!provider) return {};

    const headers: Record<string, string> = {};
    
    // Replace placeholders in header values
    Object.entries(provider.defaultHeaders).forEach(([key, value]) => {
      headers[key] = value.replace('{api_key}', apiKey);
    });
    
    return headers;
  }

  /**
   * Get provider URL with replacements
   */
  public getProviderUrl(providerId: string, params: Record<string, string> = {}): string {
    const provider = this.getProvider(providerId);
    if (!provider) return '';

    let url = provider.baseUrl;
    
    // Replace placeholders in URL
    Object.entries(params).forEach(([key, value]) => {
      url = url.replace(`{${key}}`, value);
    });
    
    return url;
  }
}
