import type { ToolHandler, ToolResult } from './types.js'
import type { ToolOutputRepository } from '../repositories/types.js'

export function registerToolOutputTools(
  registry: { register: (h: ToolHandler) => void },
  repo: ToolOutputRepository,
): void {
  registry.register({
    metadata: {
      name: 'tool_outputs.read',
      description: 'Read a persisted tool output by ID.',
      parameters: {
        type: 'object',
        properties: {
          id: { type: 'string', description: 'Tool output ID' },
        },
        required: ['id'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const id = args.id as string
      const output = await repo.getById(id)
      if (!output) {
        return { ok: false, error: `Tool output not found: ${id}` }
      }
      return { ok: true, output: output.data }
    },
  })

  registry.register({
    metadata: {
      name: 'tool_outputs.list',
      description: 'List all persisted tool outputs for an agent.',
      parameters: {
        type: 'object',
        properties: {
          agent_id: { type: 'string', description: 'Agent ID' },
        },
        required: ['agent_id'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const agentId = args.agent_id as string
      const outputs = await repo.listByAgent(agentId)
      return {
        ok: true,
        output: outputs.map((o) => ({
          id: o.id,
          tool_name: o.toolName,
          call_id: o.callId,
          created_at: o.createdAt,
        })),
      }
    },
  })

  registry.register({
    metadata: {
      name: 'tool_outputs.stats',
      description: 'Get summary statistics for a persisted tool output (type, size, keys).',
      parameters: {
        type: 'object',
        properties: {
          id: { type: 'string', description: 'Tool output ID' },
        },
        required: ['id'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const id = args.id as string
      const output = await repo.getById(id)
      if (!output) {
        return { ok: false, error: `Tool output not found: ${id}` }
      }

      const data = output.data
      const stats: Record<string, unknown> = {
        id: output.id,
        tool_name: output.toolName,
        type: typeOf(data),
        size_bytes: Buffer.byteLength(JSON.stringify(data)),
      }

      if (typeof data === 'object' && data !== null) {
        if (Array.isArray(data)) {
          stats.length = data.length
          if (data.length > 0) {
            stats.item_type = typeOf(data[0])
            if (typeof data[0] === 'object' && data[0] !== null) {
              stats.item_keys = Object.keys(data[0])
            }
          }
        } else {
          stats.keys = Object.keys(data)
        }
      }

      return { ok: true, output: stats }
    },
  })

  registry.register({
    metadata: {
      name: 'tool_outputs.extract',
      description:
        'Extract a value from a persisted tool output using dot-notation path (e.g. "data.items.0.name").',
      parameters: {
        type: 'object',
        properties: {
          id: { type: 'string', description: 'Tool output ID' },
          path: { type: 'string', description: 'Dot-notation path to extract' },
        },
        required: ['id', 'path'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const id = args.id as string
      const dotPath = args.path as string

      const output = await repo.getById(id)
      if (!output) {
        return { ok: false, error: `Tool output not found: ${id}` }
      }

      const value = extractByPath(output.data, dotPath)
      if (value === undefined) {
        return { ok: false, error: `Path not found: ${dotPath}` }
      }

      return { ok: true, output: value }
    },
  })

  registry.register({
    metadata: {
      name: 'tool_outputs.count',
      description: 'Count elements at a path in a persisted tool output.',
      parameters: {
        type: 'object',
        properties: {
          id: { type: 'string', description: 'Tool output ID' },
          path: { type: 'string', description: 'Dot-notation path (optional, counts root if empty)' },
        },
        required: ['id'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const id = args.id as string
      const dotPath = (args.path as string | undefined) ?? ''

      const output = await repo.getById(id)
      if (!output) {
        return { ok: false, error: `Tool output not found: ${id}` }
      }

      const target = dotPath ? extractByPath(output.data, dotPath) : output.data
      if (target === undefined) {
        return { ok: false, error: `Path not found: ${dotPath}` }
      }

      let count: number
      if (Array.isArray(target)) {
        count = target.length
      } else if (typeof target === 'object' && target !== null) {
        count = Object.keys(target).length
      } else {
        return { ok: false, error: 'Value at path is not an array or object' }
      }

      return { ok: true, output: { path: dotPath || '(root)', count } }
    },
  })

  registry.register({
    metadata: {
      name: 'tool_outputs.sample',
      description:
        'Sample N items from an array in a persisted tool output. Returns first, middle, and last items.',
      parameters: {
        type: 'object',
        properties: {
          id: { type: 'string', description: 'Tool output ID' },
          n: { type: 'integer', description: 'Number of items to sample (default: 3)' },
        },
        required: ['id'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const id = args.id as string
      const n = (args.n as number) ?? 3

      const output = await repo.getById(id)
      if (!output) {
        return { ok: false, error: `Tool output not found: ${id}` }
      }

      const data = output.data
      if (!Array.isArray(data)) {
        return { ok: false, error: 'Tool output data is not an array' }
      }

      if (data.length === 0) {
        return { ok: true, output: { total: 0, samples: [] } }
      }

      const samples = sampleItems(data, n)
      return {
        ok: true,
        output: {
          total: data.length,
          sampled: samples.length,
          samples,
        },
      }
    },
  })
}

function typeOf(value: unknown): string {
  if (value === null) return 'null'
  if (Array.isArray(value)) return 'array'
  return typeof value
}

function extractByPath(data: unknown, dotPath: string): unknown {
  const parts = dotPath.split('.')
  let current: unknown = data

  for (const part of parts) {
    if (current === null || current === undefined) return undefined

    if (Array.isArray(current)) {
      const idx = parseInt(part, 10)
      if (isNaN(idx)) return undefined
      current = current[idx]
    } else if (typeof current === 'object') {
      current = (current as Record<string, unknown>)[part]
    } else {
      return undefined
    }
  }

  return current
}

function sampleItems(arr: unknown[], n: number): Array<{ index: number; value: unknown }> {
  if (arr.length <= n) {
    return arr.map((value, index) => ({ index, value }))
  }

  const indices = new Set<number>()
  // Always include first and last
  indices.add(0)
  indices.add(arr.length - 1)

  // Fill remaining with evenly spaced items
  const remaining = n - indices.size
  for (let i = 0; i < remaining; i++) {
    const idx = Math.round(((i + 1) * (arr.length - 1)) / (remaining + 1))
    indices.add(idx)
  }

  return Array.from(indices)
    .sort((a, b) => a - b)
    .map((index) => ({ index, value: arr[index] }))
}
