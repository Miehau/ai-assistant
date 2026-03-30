import type {
  ToolExecutor,
  ToolHandler,
  ToolMetadata,
  ToolContext,
  ToolResult,
  ToolCall,
  ToolBatchResult,
  ValidationResult,
  ToolPreview,
} from './types.js'

export class ToolRegistryImpl implements ToolExecutor {
  private handlers = new Map<string, ToolHandler>()

  register(handler: ToolHandler): void {
    if (this.handlers.has(handler.metadata.name)) {
      throw new Error(`Tool already registered: ${handler.metadata.name}`)
    }
    this.handlers.set(handler.metadata.name, handler)
  }

  async execute(
    name: string,
    args: Record<string, unknown>,
    ctx: ToolContext,
  ): Promise<ToolResult> {
    const handler = this.handlers.get(name)
    if (!handler) {
      return { ok: false, error: `Unknown tool: ${name}` }
    }

    const validation = this.validateArgs(name, args)
    if (!validation.valid) {
      return {
        ok: false,
        error: `Validation failed: ${validation.errors!.join(', ')}.\n\nExpected parameters for "${name}":\n${formatParameterHint(handler.metadata.parameters)}`,
      }
    }

    try {
      return await handler.handle(args, ctx)
    } catch (err) {
      if (ctx.signal.aborted) {
        return { ok: false, error: 'Tool execution aborted' }
      }
      const message = err instanceof Error ? err.message : String(err)
      return { ok: false, error: message }
    }
  }

  async executeBatch(calls: ToolCall[], ctx: ToolContext): Promise<ToolBatchResult> {
    const settled = await Promise.allSettled(
      calls.map(async (call) => {
        const result = await this.execute(call.name, call.args, ctx)
        return { call_id: call.call_id, ...result }
      }),
    )

    const results = settled.map((s, i) => {
      if (s.status === 'fulfilled') return s.value
      return {
        call_id: calls[i].call_id,
        ok: false as const,
        error: s.reason instanceof Error ? s.reason.message : String(s.reason),
      }
    })

    return {
      results,
      all_ok: results.every((r) => r.ok),
    }
  }

  getMetadata(name: string): ToolMetadata | undefined {
    return this.handlers.get(name)?.metadata
  }

  listMetadata(): ToolMetadata[] {
    return Array.from(this.handlers.values()).map((h) => h.metadata)
  }

  validateArgs(name: string, args: unknown): ValidationResult {
    const handler = this.handlers.get(name)
    if (!handler) {
      return { valid: false, errors: [`Unknown tool: ${name}`] }
    }

    if (typeof args !== 'object' || args === null) {
      return { valid: false, errors: ['Args must be an object'] }
    }

    const schema = handler.metadata.parameters
    const properties = schema.properties as Record<string, Record<string, unknown>> | undefined
    const required = (schema.required as string[]) ?? []
    const errors: string[] = []
    const argObj = args as Record<string, unknown>

    for (const field of required) {
      if (argObj[field] === undefined || argObj[field] === null) {
        errors.push(`Missing required field: ${field}`)
      }
    }

    if (properties) {
      for (const [key, prop] of Object.entries(properties)) {
        const value = argObj[key]
        if (value === undefined || value === null) continue

        const expectedType = prop.type as string | undefined
        if (expectedType) {
          const actualType = Array.isArray(value) ? 'array' : typeof value
          if (expectedType === 'integer' || expectedType === 'number') {
            if (typeof value !== 'number') {
              errors.push(`Field '${key}' must be a number, got ${actualType}`)
            }
          } else if (expectedType !== actualType) {
            errors.push(`Field '${key}' must be ${expectedType}, got ${actualType}`)
          }
        }
      }
    }

    return errors.length > 0 ? { valid: false, errors } : { valid: true }
  }

  getPreview(
    name: string,
    args: Record<string, unknown>,
    ctx: ToolContext,
  ): ToolPreview | undefined {
    const handler = this.handlers.get(name)
    if (!handler?.preview) return undefined
    return handler.preview(args, ctx)
  }
}

/** Build a concise parameter hint from a JSON Schema for inclusion in error messages. */
function formatParameterHint(schema: Record<string, unknown>): string {
  const properties = schema.properties as Record<string, Record<string, unknown>> | undefined
  const required = new Set((schema.required as string[]) ?? [])

  if (!properties) return '  (no parameters defined)'

  const lines: string[] = []
  for (const [key, prop] of Object.entries(properties)) {
    const type = prop.type as string ?? 'any'
    const desc = prop.description as string ?? ''
    const req = required.has(key) ? ' (required)' : ''
    lines.push(`  - ${key}: ${type}${req}${desc ? ` — ${desc}` : ''}`)
  }
  return lines.join('\n')
}
