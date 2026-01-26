export interface OllamaModel {
  name: string;
  size: number;
  digest: string;
  modified_at: string;
}

export interface OllamaDiscoveryResult {
  models: OllamaModel[];
  available: boolean;
}
