import { OpenAIService } from '../../../openai';
import type { Message } from '$lib/types';

type PlanningResult = {
  status: 'continue' | 'done';
  nextAction?: string;
  reasoning?: string;
  context?: Record<string, any>;
};

type ExecutionResult = {
  success: boolean;
  result: string;
  context?: Record<string, any>;
};

export class ExecutorService {
  private openAIService: OpenAIService;
  private maxSteps = 5; // Prevent infinite loops

  constructor(apiKey: string) {
    this.openAIService = new OpenAIService(apiKey);
  }

  async execute(task: string, initialContext: Record<string, any> = {}): Promise<string> {
    let currentStep = 0;
    let context = { ...initialContext };
    let executionHistory: string[] = [];

    while (currentStep < this.maxSteps) {
      // Step 1: Planning Phase
      const plan = await this.plan(task, executionHistory, context);
      
      if (plan.status === 'done') {
        return `Task completed. ${plan.reasoning}`;
      }

      // Step 2: Execution Phase
      if (plan.nextAction) {
        const executionResult = await this.executeStep(plan.nextAction, context);
        executionHistory.push(`Action: ${plan.nextAction}\nResult: ${executionResult.result}`);
        
        if (!executionResult.success) {
          return `Failed to complete task. Last action: ${plan.nextAction}. Error: ${executionResult.result}`;
        }

        context = { ...context, ...executionResult.context };
      }

      currentStep++;
    }

    return `Reached maximum steps (${this.maxSteps}). Last context: ${JSON.stringify(context)}`;
  }

  private async plan(
    task: string,
    history: string[],
    context: Record<string, any>
  ): Promise<PlanningResult> {
    const systemPrompt = `
      You are a planning AI that breaks down tasks into steps.
      Based on the task, history, and context, determine if:
      1. The task is complete ('done')
      2. What the next action should be ('continue')
      
      Respond in JSON format:
      {
        "status": "continue" | "done",
        "nextAction": "description of next action (if status is continue)",
        "reasoning": "explanation of your decision",
        "context": {} // any additional context needed
      }
    `;

    const userPrompt = `
      Task: ${task}
      History: ${history.join('\n')}
      Current Context: ${JSON.stringify(context)}
    `;

    try {
      const response = await this.openAIService.createChatCompletion(
        'gpt-4o-mini',
        [
          { role: 'system', content: systemPrompt },
          { role: 'user', content: userPrompt }
        ],
        false,
        () => {},
        new AbortController().signal
      );

      return JSON.parse(response) as PlanningResult;
    } catch (error) {
      console.error('Planning failed:', error);
      throw error;
    }
  }

  private async executeStep(
    action: string,
    context: Record<string, any>
  ): Promise<ExecutionResult> {
    const systemPrompt = `
      You are an execution AI that carries out specific actions.
      Action to execute: ${action}
      Current context: ${JSON.stringify(context)}
      
      Respond in JSON format:
      {
        "success": boolean,
        "result": "description of what happened",
        "context": {} // any new context to add
      }
    `;

    try {
      const response = await this.openAIService.createChatCompletion(
        'gpt-4o-mini',
        [
          { role: 'system', content: systemPrompt },
          { role: 'user', content: 'Execute the action.' }
        ],
        false,
        () => {},
        new AbortController().signal
      );

      return JSON.parse(response) as ExecutionResult;
    } catch (error) {
      console.error('Execution failed:', error);
      return {
        success: false,
        result: error instanceof Error ? error.message : 'Unknown error occurred'
      };
    }
  }
}

// Don't initialize here - it will be initialized when needed with the API key
export const executorService = null; 