import type { ToolHandler, ToolResult } from './types.js'

export function registerThinkTool(registry: { register: (h: ToolHandler) => void }): void {
  registry.register({
    metadata: {
      name: 'think',
      description:
        "Pause and reason through what you know, what you're assuming, and what's missing. " +
        "Use this BEFORE acting to formulate an approach, and AFTER receiving results to reflect " +
        "on what worked, what failed, and what to try next. Essential for multi-step problem solving.",
      parameters: {
        type: 'object',
        properties: {
          questions: {
            type: 'array',
            items: { type: 'string' },
            description:
              "Questions you're genuinely asking yourself — what you don't know, what you're assuming, " +
              "what you're curious about, what might be worth reconsidering.",
          },
          reflection: {
            type: 'string',
            description:
              "When reflecting on a previous attempt: what did you observe? What worked, what failed, " +
              "and what does that tell you about the next approach? Leave empty on first use.",
          },
        },
        required: ['questions'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const questions = args.questions as string[]
      const reflection = typeof args.reflection === 'string' ? args.reflection.trim() : ''

      const next = reflection
        ? 'You reflected on a previous attempt. Now act: what is your next concrete step? Either try a new approach (tool call or delegate), or present the final answer if you can deduce it. Do NOT stop to report partial findings — keep going until you have a result or have exhausted options.'
        : 'Act on these questions now. For each one: answer it from what you know, or make a tool call to resolve it. IMPORTANT: after you get tool results, you MUST call think again with the reflection parameter to analyze what you learned before responding to the user. Never report raw tool output without reflecting first.'

      return {
        ok: true,
        output: {
          internal_questions: questions,
          ...(reflection ? { reflection } : {}),
          next,
        },
      }
    },
  })
}
