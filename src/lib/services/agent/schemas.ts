/**
 * JSON Schemas for agent system structured outputs
 * These replace the prompt-based JSON extraction
 */

import type { JSONSchema, StructuredOutputSchema } from '$lib/types/llm';

/**
 * Intent analysis schema
 */
export const intentAnalysisSchema: StructuredOutputSchema = {
  name: 'intent_analysis',
  description: 'Analyze user intent and determine next action',
  schema: {
    type: 'object',
    properties: {
      intent_type: {
        type: 'string',
        enum: ['tool_call', 'other'],
        description: 'Type of intent detected'
      },
      content: {
        type: 'string',
        description: 'Content to be stored or processed'
      },
      tool: {
        type: 'string',
        description: 'Name of the tool to be called, if any'
      },
      params: {
        type: 'object',
        description: 'Parameters for the tool call',
        additionalProperties: false
      },
      userMessage: {
        type: 'string',
        description: 'Message to display to the user about what is being done'
      }
    },
    required: ['intent_type'],
    additionalProperties: false
  },
  strict: true
};

/**
 * Plan schema for orchestrator
 */
export const planSchema: StructuredOutputSchema = {
  name: 'plan',
  description: 'Action plan for completing user request',
  schema: {
    type: 'object',
    properties: {
      thinking: {
        type: 'string',
        description: '1-3 sentences of inner thoughts about the task'
      },
      steps: {
        type: 'array',
        description: 'List of steps to take',
        items: {
          type: 'object',
          properties: {
            tool: {
              type: 'string',
              description: 'Exact name of the tool from available tools'
            },
            note: {
              type: 'string',
              description: 'Brief description of how to use the tool'
            }
          },
          required: ['tool', 'note'],
          additionalProperties: false
        }
      }
    },
    required: ['thinking', 'steps'],
    additionalProperties: false
  },
  strict: true
};

/**
 * Decision schema for orchestrator
 */
export const decideSchema: StructuredOutputSchema = {
  name: 'decide',
  description: 'Select next tool to execute',
  schema: {
    type: 'object',
    properties: {
      _thoughts: {
        type: 'string',
        description: '1-3 sentences about the tool you need to use'
      },
      tool: {
        type: 'string',
        description: 'Precisely pointed out name of the tool to use'
      }
    },
    required: ['_thoughts', 'tool'],
    additionalProperties: false
  },
  strict: true
};

/**
 * Tool parameters schema (dynamic based on active tool)
 */
export function createToolParametersSchema(toolName: string, toolSchema: JSONSchema): StructuredOutputSchema {
  return {
    name: `${toolName}_parameters`,
    description: `Parameters for ${toolName} tool`,
    schema: {
      type: 'object',
      properties: {
        _thoughts: {
          type: 'string',
          description: 'Internal thinking process about the values to add'
        },
        ...toolSchema.properties
      },
      required: ['_thoughts', ...(toolSchema.required || [])],
      additionalProperties: false
    },
    strict: true
  };
}

/**
 * Image preview schema
 */
export const imagePreviewSchema: StructuredOutputSchema = {
  name: 'image_preview',
  description: 'Brief description of image content',
  schema: {
    type: 'object',
    properties: {
      name: {
        type: 'string',
        description: 'Filename with extension'
      },
      preview: {
        type: 'string',
        description: 'Concise description of the image content'
      }
    },
    required: ['name', 'preview'],
    additionalProperties: false
  },
  strict: true
};

/**
 * Image context extraction schema
 */
export const imageContextSchema: StructuredOutputSchema = {
  name: 'image_context',
  description: 'Contextual information for images in article',
  schema: {
    type: 'object',
    properties: {
      images: {
        type: 'array',
        items: {
          type: 'object',
          properties: {
            name: {
              type: 'string',
              description: 'Filename with extension'
            },
            context: {
              type: 'string',
              description: '1-3 detailed sentences of context related to this image'
            }
          },
          required: ['name', 'context'],
          additionalProperties: false
        }
      }
    },
    required: ['images'],
    additionalProperties: false
  },
  strict: true
};
