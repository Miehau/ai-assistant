import type { JSONSchema } from '$lib/types/llm';

export interface ToolResult {
  success: boolean;
  result: string;
  metadata?: Record<string, any>;
}

/**
 * Tool definition with proper JSON Schema support
 * Compatible with both OpenAI and Claude structured outputs
 */
export interface Tool {
  name: string;
  description: string;

  /**
   * @deprecated Use input_schema instead
   */
  parameters?: Record<string, any>;

  /**
   * Proper JSON Schema for tool inputs
   * Must include: type, properties, required, additionalProperties
   */
  input_schema: JSONSchema;

  /**
   * Convert to schema format for LLM
   */
  toSchema: () => ToolSchema;

  /**
   * Execute the tool with validated inputs
   */
  execute: (input: any) => Promise<ToolResult>;
}

/**
 * Schema format for tool registration
 */
export interface ToolSchema {
  name: string;
  description: string;
  input_schema: JSONSchema;
  strict?: boolean; // For Claude strict mode
} 