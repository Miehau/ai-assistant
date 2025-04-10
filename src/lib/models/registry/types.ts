export interface ModelCapabilities {
  text: boolean;
  vision: boolean;
  audio: boolean;
  embedding: boolean;
  function_calling: boolean;
  reasoning?: boolean;
  image_generation?: boolean;
  transcription?: boolean;
}

export interface ModelSpecs {
  contextWindow: number;
  maxOutputTokens?: number;
  tokenLimit?: number;
  tokenization: {
    specialTokens?: {
      system?: string[];
      user?: string[];
      assistant?: string[];
      function?: string[];
    };
    encoding?: string;
    averageTokensPerChar?: number;
  };
}

export interface ModelParameters {
  temperature?: number;
  top_p?: number;
  frequency_penalty?: number;
  presence_penalty?: number;
  [key: string]: any;
}

export interface ModelMetadata {
  releaseDate?: string;
  version?: string;
  deprecated?: boolean;
  tags?: string[];
}

export interface ModelConfig {
  id: string;
  name: string;
  provider: string;
  capabilities: ModelCapabilities;
  specs: ModelSpecs;
  defaultParameters: ModelParameters;
  metadata?: ModelMetadata;
}

export interface ProviderConfig {
  id: string;
  name: string;
  authType: "api_key" | "oauth";
  baseUrl: string;
  defaultHeaders: Record<string, string>;
}

export interface ProvidersConfig {
  providers: ProviderConfig[];
}

export interface ModelsConfig {
  models: ModelConfig[];
}
