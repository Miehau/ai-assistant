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
  console.warn('[modelManagement] getModels not yet implemented in server backend');
  return [];
}

export async function addModel(model: Omit<Model, 'id'>): Promise<void> {
  throw new Error('Not yet implemented in server backend');
}
