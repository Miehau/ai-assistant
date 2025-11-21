import type { Tool, ToolResult } from './types';

export class FinalAnswerTool implements Tool {
  name = 'final_answer';
  description = 'Use this tool when you have all information needed to provide final answer to the user.';

  /** @deprecated Use input_schema instead */
  parameters = {
    answer: {
      type: 'string',
      description: 'The final answer to provide to the user'
    }
  };

  // Proper JSON Schema format
  input_schema = {
    type: 'object' as const,
    properties: {
      answer: {
        type: 'string' as const,
        description: 'The final answer to provide to the user'
      }
    },
    required: ['answer'],
    additionalProperties: false
  };

  toSchema() {
    return {
      name: this.name,
      description: this.description,
      input_schema: this.input_schema,
      strict: true
    };
  }

  async execute(params: Record<string, any>): Promise<ToolResult> {
    const answer = params.answer;
    
    if (!answer || typeof answer !== 'string') {
      return {
        success: false,
        result: 'Answer parameter must be a non-empty string'
      };
    }

    return {
      success: true,
      result: answer
    };
  }
}

export const finalAnswer = new FinalAnswerTool();