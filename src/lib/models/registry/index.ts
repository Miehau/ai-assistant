export * from './types';
export * from './modelRegistryService';

// Create and export a singleton instance
import { ModelRegistryService } from './modelRegistryService';
export const modelRegistry = new ModelRegistryService();
