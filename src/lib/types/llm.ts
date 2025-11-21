import type { z } from 'zod';

/**
 * Provider-agnostic message types
 * Replaces OpenAI-specific ChatCompletionMessageParam
 */
export type LLMMessageRole = 'system' | 'user' | 'assistant' | 'tool';

export interface LLMTextContent {
  type: 'text';
  text: string;
}

export interface LLMImageContent {
  type: 'image_url';
  image_url: {
    url: string;
    detail?: 'low' | 'high' | 'auto';
  };
}

export type LLMMessageContent = string | Array<LLMTextContent | LLMImageContent>;

export interface LLMMessage {
  role: LLMMessageRole;
  content: LLMMessageContent;
  name?: string;
  tool_call_id?: string;
}

/**
 * JSON Schema definition for structured outputs
 */
export interface JSONSchema {
  type: 'object' | 'string' | 'number' | 'boolean' | 'array' | 'null';
  properties?: Record<string, JSONSchema>;
  items?: JSONSchema;
  required?: string[];
  enum?: Array<string | number>;
  description?: string;
  additionalProperties?: boolean;
  format?: 'date-time' | 'date' | 'email' | 'uri' | 'uuid' | 'ipv4' | 'ipv6';
}

/**
 * Structured output configuration
 */
export interface StructuredOutputSchema {
  name?: string;
  description?: string;
  schema: JSONSchema;
  strict?: boolean; // For Claude's strict tool use
}

/**
 * Provider-specific structured output formats
 */
export type OpenAIStructuredOutput = {
  type: 'json_schema';
  json_schema: {
    name: string;
    schema: JSONSchema;
    strict?: boolean;
  };
};

export type ClaudeStructuredOutput = {
  type: 'json_schema';
  schema: JSONSchema;
};

/**
 * Completion options
 */
export interface LLMCompletionOptions {
  temperature?: number;
  max_tokens?: number;
  top_p?: number;
  stream?: boolean;
  stop?: string[];
  signal?: AbortSignal;
  structuredOutput?: StructuredOutputSchema;
}

/**
 * Token usage statistics
 */
export interface LLMUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

/**
 * LLM completion response
 */
export interface LLMResponse {
  message: string;
  role: string;
  usage?: LLMUsage;
  finishReason?: 'stop' | 'length' | 'content_filter' | 'refusal' | 'tool_calls';
}

/**
 * Streaming response chunk
 */
export interface LLMStreamChunk {
  delta: string;
  finishReason?: string;
}

/**
 * Helper to convert Zod schema to JSON Schema
 */
export function zodToJsonSchema(zodSchema: z.ZodType): JSONSchema {
  // This is a simplified version - you may want to use a library like zod-to-json-schema
  // For now, we'll implement basic support
  const def = (zodSchema as any)._def;

  if (def.typeName === 'ZodObject') {
    const properties: Record<string, JSONSchema> = {};
    const required: string[] = [];

    for (const [key, value] of Object.entries(def.shape())) {
      properties[key] = zodToJsonSchema(value as z.ZodType);
      if (!(value as any).isOptional()) {
        required.push(key);
      }
    }

    return {
      type: 'object',
      properties,
      required: required.length > 0 ? required : undefined,
      additionalProperties: false
    };
  }

  if (def.typeName === 'ZodString') {
    return { type: 'string', description: def.description };
  }

  if (def.typeName === 'ZodNumber') {
    return { type: 'number', description: def.description };
  }

  if (def.typeName === 'ZodBoolean') {
    return { type: 'boolean', description: def.description };
  }

  if (def.typeName === 'ZodArray') {
    return {
      type: 'array',
      items: zodToJsonSchema(def.type),
      description: def.description
    };
  }

  if (def.typeName === 'ZodEnum') {
    return {
      type: 'string',
      enum: def.values,
      description: def.description
    };
  }

  // Default fallback
  return { type: 'object', additionalProperties: false };
}
