import type {
  LLMMessage,
  LLMResponse,
  LLMCompletionOptions,
  StructuredOutputSchema
} from '$lib/types/llm';

/**
 * Abstract base class for all LLM service providers
 * Provides a unified interface for OpenAI, Claude, DeepSeek, etc.
 */
export abstract class LLMService {
  protected apiKey: string;

  constructor(apiKey: string) {
    this.apiKey = apiKey;
  }

  /**
   * Standard completion method - returns text
   */
  abstract completion(
    model: string,
    messages: LLMMessage[],
    options?: LLMCompletionOptions
  ): Promise<LLMResponse>;

  /**
   * Structured completion - returns validated JSON matching schema
   */
  abstract structuredCompletion<T = any>(
    model: string,
    messages: LLMMessage[],
    schema: StructuredOutputSchema,
    options?: Omit<LLMCompletionOptions, 'structuredOutput'>
  ): Promise<LLMStructuredResponse<T>>;

  /**
   * Audio transcription
   */
  abstract transcribeAudio(
    base64Audio: string,
    context: string
  ): Promise<string>;

  /**
   * Get provider name
   */
  abstract get providerName(): string;

  /**
   * Check if provider supports structured outputs
   */
  abstract get supportsStructuredOutputs(): boolean;

  /**
   * Validate that a schema is supported by this provider
   */
  protected validateSchema(schema: StructuredOutputSchema): void {
    if (!schema.schema) {
      throw new Error('Schema is required for structured outputs');
    }

    if (schema.schema.type !== 'object') {
      throw new Error('Root schema type must be "object"');
    }

    if (!schema.schema.properties) {
      throw new Error('Schema must define properties');
    }

    // Check for unsupported features
    this.checkSchemaFeatures(schema.schema);
  }

  /**
   * Recursively check schema for unsupported features
   */
  private checkSchemaFeatures(schema: any): void {
    // Check for recursive schemas
    if (schema.$ref) {
      throw new Error('Recursive schemas ($ref) are not supported');
    }

    // Check for numerical constraints (may not be supported by all providers)
    if (schema.minimum !== undefined || schema.maximum !== undefined) {
      console.warn('Numerical constraints (minimum/maximum) may not be enforced by all providers');
    }

    // Recursively check nested objects
    if (schema.properties) {
      for (const prop of Object.values(schema.properties)) {
        this.checkSchemaFeatures(prop);
      }
    }

    if (schema.items) {
      this.checkSchemaFeatures(schema.items);
    }
  }
}

/**
 * Structured response with typed data
 */
export interface LLMStructuredResponse<T = any> {
  data: T;
  rawResponse: string;
  usage?: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
  refusal?: string; // If the model refused to answer
}

/**
 * Error types for LLM services
 */
export class LLMServiceError extends Error {
  constructor(
    message: string,
    public provider: string,
    public originalError?: unknown
  ) {
    super(message);
    this.name = 'LLMServiceError';
  }
}

export class SchemaValidationError extends LLMServiceError {
  constructor(
    message: string,
    provider: string,
    public schema: StructuredOutputSchema,
    originalError?: unknown
  ) {
    super(message, provider, originalError);
    this.name = 'SchemaValidationError';
  }
}

export class RefusalError extends LLMServiceError {
  constructor(
    message: string,
    provider: string,
    public refusalReason: string
  ) {
    super(message, provider);
    this.name = 'RefusalError';
  }
}
