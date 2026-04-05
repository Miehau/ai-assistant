import type { ToolHandler } from './types.js'
import type { AgentDefinitionRegistry } from '../agents/registry.js'
import type { InterceptHandler } from '../orchestrator/types.js'
import { handleDelegation } from '../orchestrator/runner.js'

/**
 * Delegate tool — spawns a child agent to handle a subtask.
 *
 * Marked `orchestrator_intercept: true` — the runner intercepts this tool
 * and calls the registered InterceptHandler instead of handle(). The handle()
 * method exists only as a safety net if interception is bypassed.
 *
 * Named agents defined in server/agents/*.md are listed in the tool
 * description so the LLM knows which specialised agents are available.
 */
export function registerDelegateTools(
  registry: { register: (h: ToolHandler) => void },
  agentDefs: AgentDefinitionRegistry,
  interceptHandlers?: Map<string, InterceptHandler>,
): void {
  const named = agentDefs.list().filter((d) => d.name !== 'default')

  let description =
    'Delegate a subtask to a child agent. Good for: multi-step research, searches that produce large outputs, or independent subtasks that would pollute your context. Not for: single API calls, simple web fetches, or any operation you can accomplish in one tool call — do those directly.'

  if (named.length > 0) {
    const lines = named.map((d) => `- ${d.name}: ${d.description ?? 'no description'}`)
    description += `\n\nAvailable named agents (pass via the "agent" parameter):\n${lines.join('\n')}`
  }

  const agentNames = named.map((d) => d.name)

  registry.register({
    metadata: {
      name: 'delegate',
      description,
      parameters: {
        type: 'object',
        properties: {
          task: {
            type: 'string',
            description: 'Clear description of what the child agent should accomplish',
          },
          ...(agentNames.length > 0
            ? {
                agent: {
                  type: 'string',
                  enum: agentNames,
                  description: `Named agent to use. If omitted, the default general-purpose agent is used. Available: ${agentNames.join(', ')}`,
                },
              }
            : {}),
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

  // Self-register the intercept handler
  interceptHandlers?.set('delegate', handleDelegation)
}
