import type { NodeSDK } from '@opentelemetry/sdk-node'
import type { propagateAttributes, startActiveObservation } from '@langfuse/tracing'
import type { AppConfig } from '../lib/config.js'
import { logger } from '../lib/logger.js'
import type { LLMMessage, LLMResponse } from '../providers/types.js'
import type {
  AgentTraceContext,
  GenerationTraceContext,
  LLMObservability,
  ToolTraceContext,
} from './types.js'

const SECRET_PATTERNS = [
  /\b(sk-[A-Za-z0-9_-]{12,})\b/g,
  /\b(pk-[A-Za-z0-9_-]{12,})\b/g,
  /\b(api[_-]?key|authorization|bearer|token|password|secret)\s*[:=]\s*["']?[^"'\s,}]+/gi,
]

class NoopObservability implements LLMObservability {
  readonly enabled = false

  traceAgent<T extends { status: string; error?: string; result?: string; turnCount?: number }>(
    _context: AgentTraceContext,
    fn: () => Promise<T>,
  ): Promise<T> {
    return fn()
  }

  traceGeneration<T extends LLMResponse>(_context: GenerationTraceContext, fn: () => Promise<T>): Promise<T> {
    return fn()
  }

  traceTool<T>(_context: ToolTraceContext, fn: () => Promise<T>): Promise<T> {
    return fn()
  }

  async shutdown(): Promise<void> {}
}

export async function createLangfuseObservability(config: AppConfig): Promise<LLMObservability> {
  const shouldEnable = config.langfuseEnabled ?? Boolean(config.langfusePublicKey && config.langfuseSecretKey)
  if (!shouldEnable) {
    return new NoopObservability()
  }

  if (!config.langfusePublicKey || !config.langfuseSecretKey) {
    logger.warn('Langfuse tracing disabled: LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY are required')
    return new NoopObservability()
  }

  try {
    const [{ NodeSDK }, { LangfuseSpanProcessor }, tracing] = await Promise.all([
      import('@opentelemetry/sdk-node'),
      import('@langfuse/otel'),
      import('@langfuse/tracing'),
    ])

    const sdk = new NodeSDK({
      spanProcessors: [
        new LangfuseSpanProcessor({
          publicKey: config.langfusePublicKey,
          secretKey: config.langfuseSecretKey,
          baseUrl: config.langfuseBaseUrl,
          environment: config.nodeEnv,
          mask: ({ data }) => scrub(data, Math.max(200, config.langfuseMaxContentChars)),
        }),
      ],
    })
    sdk.start()
    return new LangfuseObservability(config, sdk, tracing.startActiveObservation, tracing.propagateAttributes)
  } catch (err) {
    logger.error({ err }, 'Langfuse tracing disabled: required packages could not be loaded')
    return new NoopObservability()
  }
}

class LangfuseObservability implements LLMObservability {
  readonly enabled = true
  private readonly sdk: NodeSDK
  private readonly captureContent: boolean
  private readonly maxContentChars: number
  private readonly startActiveObservation: typeof startActiveObservation
  private readonly propagateAttributes: typeof propagateAttributes

  constructor(
    config: AppConfig,
    sdk: NodeSDK,
    start: typeof startActiveObservation,
    propagate: typeof propagateAttributes,
  ) {
    this.captureContent = config.langfuseCaptureContent
    this.maxContentChars = Math.max(200, config.langfuseMaxContentChars)
    this.sdk = sdk
    this.startActiveObservation = start
    this.propagateAttributes = propagate
    logger.info({ baseUrl: config.langfuseBaseUrl, captureContent: this.captureContent }, 'Langfuse tracing enabled')
  }

  async traceAgent<T extends { status: string; error?: string; result?: string; turnCount?: number }>(
    context: AgentTraceContext,
    fn: () => Promise<T>,
  ): Promise<T> {
    return this.startActiveObservation(
      'agent.run',
      async (agentSpan) => {
        const metadata = {
          agentId: context.agent.id,
          parentId: context.agent.parentId,
          sourceCallId: context.agent.sourceCallId,
          depth: context.agent.depth,
          provider: context.agent.config.provider,
          model: context.agent.config.model,
          maxTurns: context.agent.config.max_turns,
          responseFormat: context.agent.config.response_format ?? 'markdown',
          inputSource: context.inputSource ?? 'agent_task',
        }
        agentSpan.update({
          input: summarizeValue(context.input ?? context.agent.task, this.captureContent, this.maxContentChars),
          metadata,
        })

        return this.propagateAttributes(
          {
            sessionId: context.agent.sessionId,
            traceName: `agent:${context.agent.id}`,
            tags: ['agent-run', context.agent.config.provider],
            metadata: {
              agentId: context.agent.id,
              provider: context.agent.config.provider,
              model: context.agent.config.model.slice(0, 200),
            },
          },
          async () => {
            try {
              const result = await fn()
              agentSpan.update({
                output: summarizeValue(result.result ?? result.error ?? result.status, this.captureContent, this.maxContentChars),
                level: result.status === 'failed' || result.status === 'cancelled' ? 'ERROR' : 'DEFAULT',
                statusMessage: result.error,
                metadata: {
                  ...metadata,
                  status: result.status,
                  turnCount: result.turnCount,
                },
              })
              return result
            } catch (err) {
              agentSpan.update({
                level: 'ERROR',
                statusMessage: errorMessage(err),
              })
              throw err
            }
          },
        )
      },
      { asType: 'agent' },
    )
  }

  async traceGeneration<T extends LLMResponse>(
    context: GenerationTraceContext,
    fn: () => Promise<T>,
  ): Promise<T> {
    return this.startActiveObservation(
      'llm.generation',
      async (generation) => {
        const input = summarizeRequest(context.request, this.captureContent, this.maxContentChars)
        const metadata = {
          agentId: context.agent.id,
          provider: context.provider,
          turnNumber: context.turnNumber,
          stream: context.stream,
          parentId: context.agent.parentId,
          depth: context.agent.depth,
        }
        generation.update({
          input,
          model: context.model,
          modelParameters: modelParameters(context.request),
          metadata,
        })

        return this.propagateAttributes(
          {
            sessionId: context.agent.sessionId,
            metadata: {
              agentId: context.agent.id,
              provider: context.provider,
              model: context.model.slice(0, 200),
            },
          },
          async () => {
            try {
              const response = await fn()
              generation.update({
                output: summarizeResponse(response, this.captureContent, this.maxContentChars),
                usageDetails: usageDetails(response),
                metadata: {
                  ...metadata,
                  finishReason: response.finish_reason,
                  hasToolCalls: Boolean(response.tool_calls?.length),
                },
              })
              return response
            } catch (err) {
              generation.update({
                level: 'ERROR',
                statusMessage: errorMessage(err),
              })
              throw err
            }
          },
        )
      },
      { asType: 'generation' },
    )
  }

  async traceTool<T>(context: ToolTraceContext, fn: () => Promise<T>): Promise<T> {
    return this.startActiveObservation(
      `tool.${context.name}`,
      async (tool) => {
        const metadata = {
          agentId: context.agent.id,
          callId: context.callId,
          toolName: context.name,
          approved: context.approved ?? false,
          parentId: context.agent.parentId,
          depth: context.agent.depth,
        }
        tool.update({
          input: summarizeValue(context.args, this.captureContent, this.maxContentChars),
          metadata,
        })

        return this.propagateAttributes(
          {
            sessionId: context.agent.sessionId,
            metadata: {
              agentId: context.agent.id,
              toolName: context.name.slice(0, 200),
            },
          },
          async () => {
            try {
              const result = await fn()
              const ok = isToolLikeResult(result) ? result.ok : true
              tool.update({
                output: summarizeValue(result, this.captureContent, this.maxContentChars),
                level: ok ? 'DEFAULT' : 'ERROR',
                metadata: {
                  ...metadata,
                  ok,
                },
              })
              return result
            } catch (err) {
              tool.update({
                level: 'ERROR',
                statusMessage: errorMessage(err),
              })
              throw err
            }
          },
        )
      },
      { asType: 'tool' },
    )
  }

  async shutdown(): Promise<void> {
    try {
      await this.sdk.shutdown()
    } catch (err) {
      logger.warn({ err }, 'Langfuse tracing shutdown failed')
    }
  }
}

function modelParameters(request: GenerationTraceContext['request']): Record<string, string | number> {
  const params: Record<string, string | number> = {}
  if (request.temperature !== undefined) params.temperature = request.temperature
  if (request.max_tokens !== undefined) params.max_tokens = request.max_tokens
  if (request.tools?.length) params.tool_count = request.tools.length
  if (request.structured_output) params.structured_output = 'true'
  return params
}

function usageDetails(response: LLMResponse): Record<string, number> {
  const input = response.usage.input_tokens
  const output = response.usage.output_tokens
  const cacheRead = response.usage.cache_read_input_tokens ?? 0
  const cacheCreation = response.usage.cache_creation_input_tokens ?? 0
  return {
    promptTokens: input,
    completionTokens: output,
    totalTokens: input + output + cacheRead + cacheCreation,
    ...(cacheRead > 0 ? { cacheReadInputTokens: cacheRead } : {}),
    ...(cacheCreation > 0 ? { cacheCreationInputTokens: cacheCreation } : {}),
  }
}

function summarizeRequest(request: GenerationTraceContext['request'], capture: boolean, maxChars: number): unknown {
  return {
    model: request.model,
    messageCount: request.messages.length,
    roles: request.messages.map((message) => message.role),
    toolCount: request.tools?.length ?? 0,
    hasStructuredOutput: Boolean(request.structured_output),
    ...(capture ? { messages: summarizeMessages(request.messages, maxChars) } : {}),
  }
}

function summarizeResponse(response: LLMResponse, capture: boolean, maxChars: number): unknown {
  return {
    finishReason: response.finish_reason,
    hasToolCalls: Boolean(response.tool_calls?.length),
    toolCalls: response.tool_calls?.map((toolCall) => ({
      name: toolCall.name,
      callId: toolCall.call_id,
    })),
    usage: response.usage,
    ...(capture ? { content: summarizeValue(response.content, true, maxChars) } : {}),
  }
}

function summarizeMessages(messages: LLMMessage[], maxChars: number): unknown {
  return messages.map((message) => ({
    role: message.role,
    toolCallId: message.tool_call_id,
    content: summarizeValue(message.content, true, maxChars),
    toolCalls: message.tool_calls?.map((toolCall) => ({
      name: toolCall.name,
      callId: toolCall.call_id,
    })),
  }))
}

function summarizeValue(value: unknown, capture: boolean, maxChars: number): unknown {
  if (!capture) return summarizeShape(value)
  return scrub(value, maxChars)
}

function summarizeShape(value: unknown): unknown {
  if (value == null) return value
  if (typeof value === 'string') return { type: 'string', chars: value.length }
  if (typeof value === 'number' || typeof value === 'boolean') return value
  if (Array.isArray(value)) return { type: 'array', length: value.length }
  if (typeof value === 'object') return { type: 'object', keys: Object.keys(value as Record<string, unknown>).slice(0, 20) }
  return { type: typeof value }
}

function scrub(value: unknown, maxChars: number): unknown {
  if (typeof value === 'string') return truncate(redact(value), maxChars)
  if (Array.isArray(value)) return value.map((item) => scrub(item, maxChars))
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {}
    for (const [key, raw] of Object.entries(value)) {
      if (isSensitiveKey(key)) {
        out[key] = '[REDACTED]'
      } else if (isLargeBinaryString(raw)) {
        out[key] = '[BINARY_REDACTED]'
      } else {
        out[key] = scrub(raw, maxChars)
      }
    }
    return out
  }
  return value
}

function redact(value: string): string {
  let result = value
  for (const pattern of SECRET_PATTERNS) {
    result = result.replace(pattern, (match) => {
      const [prefix] = match.split(/[:=]/)
      return prefix && prefix !== match ? `${prefix}: [REDACTED]` : '[REDACTED]'
    })
  }
  return result
}

function truncate(value: string, maxChars: number): string {
  return value.length <= maxChars ? value : `${value.slice(0, maxChars)}...[truncated ${value.length - maxChars} chars]`
}

function isSensitiveKey(key: string): boolean {
  return /api[_-]?key|authorization|bearer|token|password|secret|credential/i.test(key)
}

function isLargeBinaryString(value: unknown): boolean {
  return typeof value === 'string' && value.length > 1024 && /^[A-Za-z0-9+/=]+$/.test(value)
}

function isToolLikeResult(value: unknown): value is { ok: boolean } {
  return Boolean(value && typeof value === 'object' && 'ok' in value && typeof (value as { ok?: unknown }).ok === 'boolean')
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}
