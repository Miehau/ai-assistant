import type { Tool, ToolResult } from './types';

export class FinalAnswerTool implements Tool {
  name = 'final_answer';
  description = 'Use this tool when you have all information needed to provide final answer to the user.';
  parameters = {
    answer: {
      type: 'string',
      description: 'The final answer to provide to the user'
    }
  };

  toSchema() {
    return JSON.stringify({
      "name": this.name,
      "description": this.description,
      "parameters": this.parameters
    });
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