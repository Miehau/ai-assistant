import { invoke } from '@tauri-apps/api/tauri';
import { v4 as uuidv4 } from 'uuid';

export interface Model {
  id: string;
  provider: string;
  apiKey?: string;
  modelName: string;
  url?: string;
  deploymentName?: string;
}

export async function getModels(): Promise<Model[]> {
  try {
    const models: Model[] = await invoke('get_models');
    return models;
  } catch (error) {
    console.error('Failed to get models:', error);
    throw new Error('Failed to get models');
  }
}

export async function addModel(model: Omit<Model, 'id'>): Promise<void> {
  try {
    const modelWithId: Model = {
      ...model,
      id: uuidv4()
    };
    await invoke('add_model', { model: modelWithId });
  } catch (error) {
    console.error('Failed to add model:', error);
    throw new Error('Failed to add model');
  }
}