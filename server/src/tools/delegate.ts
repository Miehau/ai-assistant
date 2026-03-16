import type { ToolHandler } from './types.js'

/**
 * Delegate tool — spawns a child agent to handle a subtask.
 *
 * Marked `orchestrator_intercept: true` — the runner intercepts this tool
 * and runs handleDelegation() instead of calling handle(). The handle()
 * method exists only as a safety net if interception is bypassed.
 */
export function registerDelegateTools(registry: { register: (h: ToolHandler) => void }): void {
  registry.register({
    metadata: {
      name: 'delegate',
      description:
        'Delegate a subtask to a child agent. The child agent runs with the same tools and model. Use this when a task can be broken into independent subtasks that benefit from separate context.',
      parameters: {
        type: 'object',
        properties: {
          task: {
            type: 'string',
            description: 'Clear description of what the child agent should accomplish',
          },
        },
        required: ['task'],
      },
      requires_approval: false,
      orchestrator_intercept: true,
    },
    async handle() {
      return { ok: false, error: 'delegate must be intercepted by the orchestrator — this is a bug' }
    },
  })
}
