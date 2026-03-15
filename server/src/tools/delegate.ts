import type { ToolHandler } from './types.js'

/**
 * Delegate tool — spawns a child agent to handle a subtask.
 * The handler is a no-op validator; the runner intercepts tools with
 * name === 'delegate' and runs handleDelegation() instead.
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
    },
    async handle(args) {
      // Validation only — real spawning happens in runner's handleDelegation()
      const task = (args as Record<string, unknown>).task as string
      if (!task) {
        return { ok: false, error: '"task" is required' }
      }
      return { ok: true, output: JSON.stringify({ task }) }
    },
  })
}
