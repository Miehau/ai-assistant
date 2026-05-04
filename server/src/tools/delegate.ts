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
  const named = agentDefs.list().filter((d) => !['default', 'planner'].includes(d.name))

  let description = 'Delegate substantial or specialist work to a child agent. Do not use for simple lookups or single tool calls.'

  if (named.length > 0) {
    description += ` Agents: ${named.map((d) => d.name).join(', ')}.`
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
            description: 'Self-contained task brief',
          },
          ...(agentNames.length > 0
            ? {
                agent: {
                  type: 'string',
                  enum: agentNames,
                  description: 'Specialist agent name',
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
