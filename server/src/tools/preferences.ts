import type { ToolHandler, ToolResult } from './types.js'
import type { PreferenceRepository } from '../repositories/types.js'

export function registerPreferenceTools(
  registry: { register: (h: ToolHandler) => void },
  repo: PreferenceRepository,
): void {
  registry.register({
    metadata: {
      name: 'preferences.get',
      description: 'Get preference.',
      parameters: {
        type: 'object',
        properties: {
          key: { type: 'string', description: 'Key' },
        },
        required: ['key'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const key = args.key as string
      const value = await repo.get(key)
      if (value === null) {
        return { ok: true, output: { key, value: null, found: false } }
      }
      return { ok: true, output: { key, value, found: true } }
    },
  })

  registry.register({
    metadata: {
      name: 'preferences.set',
      description: 'Set preference.',
      parameters: {
        type: 'object',
        properties: {
          key: { type: 'string', description: 'Key' },
          value: { type: 'string', description: 'Value' },
        },
        required: ['key', 'value'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const key = args.key as string
      const value = args.value as string
      await repo.set(key, value)
      return { ok: true, output: { key, value, saved: true } }
    },
  })
}
