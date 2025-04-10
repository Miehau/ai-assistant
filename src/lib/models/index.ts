export * from './registry';
import { ApiKeyService } from './apiKeyService';
import { ModelService } from './modelService';

// Create and export singleton instances
export const apiKeyService = new ApiKeyService();
export const modelService = new ModelService();
