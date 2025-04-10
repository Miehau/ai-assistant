// Debug utility for logging store values
import { get } from 'svelte/store';
import { availableModels } from './chat/store';
import { modelRegistry } from '$lib/models';

export function debugModels() {
  console.log('Available models in store:', get(availableModels));
  console.log('Models in registry:', modelRegistry.getAllModels());
  return {
    storeModels: get(availableModels),
    registryModels: modelRegistry.getAllModels()
  };
}
