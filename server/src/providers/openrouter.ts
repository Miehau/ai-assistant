import { logger } from '../lib/logger.js'
import type {
  LLMProvider,
  LLMRequest,
  LLMResponse,
  LLMStreamEvent,
  LLMToolCall,
} from './types.js'

const BASE_URL = 'https://openrouter.ai/api/v1/chat/completions'

function buildTokenLimit(model: string, maxTokens?: number): Record<string, number> {
  return { max_tokens: 4068 }
}

/** Replace dots with __ so tool names satisfy OpenAI's ^[a-zA-Z0-9_-]{1,64}$ requirement. */
function sanitizeToolName(name: string): string {
  return name.replace(/\./g, '__')
}

/** Reverse sanitizeToolName — restores original dot-separated names. */
function restoreToolName(name: string): string {
  return name.replace(/__/g, '.')
}

/**
 * OpenRouter provider — raw fetch against the OpenAI-compatible
 * /api/v1/chat/completions endpoint. Non-streaming only for simplicity.
 */
export class OpenRouterProvider implements LLMProvider {
  private apiKey: string

  constructor(apiKey: string) {
    this.apiKey = apiKey
  }

  async generate(request: LLMRequest): Promise<LLMResponse> {
    const body = buildRequestBody(request)

    logger.debug({ provider: 'openrouter', model: request.model, messages: body.messages }, 'LLM request messages')

    const res = await fetch(BASE_URL, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
        'HTTP-Referer': 'https://ai-frontend.app',
        'X-Title': 'AI Frontend',
      },
      body: JSON.stringify(body),
      signal: request.signal,
    })

    if (!res.ok) {
      const err = await res.text()
      throw new Error(`OpenRouter ${res.status}: ${err}`)
    }

    const json = (await res.json()) as ChatCompletionResponse

    logger.debug({ provider: 'openrouter', model: request.model, raw: json }, 'Raw LLM response')

    return mapResponse(json)
  }

  async *stream(request: LLMRequest): AsyncIterable<LLMStreamEvent> {
    const response = await this.generate(request)

    if (response.content) {
      yield { type: 'text_delta', text: typeof response.content === 'string' ? response.content : String(response.content) }
      yield { type: 'text_done', text: typeof response.content === 'string' ? response.content : String(response.content) }
    }

    yield { type: 'done', response }
  }
}

// ---------------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------------

function buildRequestBody(request: LLMRequest): Record<string, unknown> {
  const messages: Record<string, unknown>[] = []
  let deferredImages: Array<{ type: string; data?: string; media_type?: string }> = []

  for (const msg of request.messages) {
    if (msg.role === 'system') {
      messages.push({
        role: 'system',
        content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content),
      })
      continue
    }

    if (msg.role === 'tool') {
      if (Array.isArray(msg.content)) {
        // OpenRouter (OpenAI-compat) tool role only accepts string — extract text for the tool result.
        // Defer image blocks to be injected after all consecutive tool messages to avoid breaking
        // the tool message chain for batch tool calls.
        const textContent = (msg.content as Array<{ type: string; text?: string }>)
          .filter((b) => b.type === 'text')
          .map((b) => b.text ?? '')
          .join('\n') || '(tool result)'
        const imageBlocks = (msg.content as Array<{ type: string; data?: string; media_type?: string }>)
          .filter((b) => b.type === 'image' && b.data)
        if (imageBlocks.length > 0) {
          deferredImages.push(...imageBlocks)
        }
        messages.push({ role: 'tool', tool_call_id: msg.tool_call_id, content: textContent })
      } else {
        messages.push({
          role: 'tool',
          tool_call_id: msg.tool_call_id,
          content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content),
        })
      }
      continue
    }

    // Flush deferred images once the tool message chain ends
    if (deferredImages.length > 0) {
      messages.push({
        role: 'user',
        content: deferredImages.map((b) => ({
          type: 'image_url',
          image_url: { url: `data:${b.media_type ?? 'image/png'};base64,${b.data}` },
        })),
      })
      deferredImages = []
    }

    if (msg.role === 'assistant') {
      const tool_calls = msg.tool_calls?.map((tc) => ({
        id: tc.call_id,
        type: 'function',
        function: {
          name: sanitizeToolName(tc.name),
          arguments: JSON.stringify(tc.arguments),
        },
      }))

      // Use null for empty content when tool_calls are present — OpenAI-compat APIs
      // may misinterpret empty string as the assistant having spoken.
      const assistantContent = typeof msg.content === 'string' && msg.content ? msg.content : null
      messages.push({
        role: 'assistant',
        content: assistantContent,
        ...(tool_calls?.length && { tool_calls }),
      })
      continue
    }

    // user message
    if (typeof msg.content === 'string') {
      messages.push({ role: 'user', content: msg.content })
    } else if (Array.isArray(msg.content)) {
      const parts = msg.content.map((block) => {
        if (block.type === 'text' && block.text) {
          return { type: 'text', text: block.text }
        }
        if (block.type === 'image' && block.data) {
          return {
            type: 'image_url',
            image_url: {
              url: `data:${block.media_type ?? 'image/png'};base64,${block.data}`,
            },
          }
        }
        return null
      }).filter(Boolean)

      messages.push({ role: 'user', content: parts })
    } else {
      messages.push({ role: 'user', content: '' })
    }
  }

  // Flush any remaining deferred images (e.g. if the last messages were tool results)
  if (deferredImages.length > 0) {
    messages.push({
      role: 'user',
      content: deferredImages.map((b) => ({
        type: 'image_url',
        image_url: { url: `data:${b.media_type ?? 'image/png'};base64,${b.data}` },
      })),
    })
  }

  const tools = request.tools?.length
    ? request.tools.map((t) => ({
        type: 'function',
        function: {
          name: sanitizeToolName(t.name),
          description: t.description,
          parameters: t.parameters,
        },
      }))
    : undefined

  return {
    model: request.model,
    messages,
    ...(tools && { tools }),
    ...(request.temperature !== undefined && { temperature: request.temperature }),
    ...buildTokenLimit(request.model, request.max_tokens),
    ...(request.structured_output && {
      response_format: {
        type: 'json_schema',
        json_schema: {
          name: 'response',
          schema: request.structured_output,
          strict: true,
        },
      },
    }),
  }
}

// ---------------------------------------------------------------------------
// Response mapping
// ---------------------------------------------------------------------------

function mapResponse(json: ChatCompletionResponse): LLMResponse {
  const choice = json.choices?.[0]

  if (!choice) {
    return {
      content: '',
      usage: {
        input_tokens: json.usage?.prompt_tokens ?? 0,
        output_tokens: json.usage?.completion_tokens ?? 0,
      },
      finish_reason: 'stop',
    }
  }

  const text = choice.message.content ?? choice.message.reasoning_content ?? ''

  const toolCalls: LLMToolCall[] = (choice.message.tool_calls ?? []).map((tc) => ({
    call_id: tc.id,
    name: restoreToolName(tc.function.name),
    arguments: tc.function.arguments ? JSON.parse(tc.function.arguments) : {},
  }))

  return {
    content: text,
    companion_text: toolCalls.length > 0 && text ? text : undefined,
    tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
    usage: {
      input_tokens: json.usage?.prompt_tokens ?? 0,
      output_tokens: json.usage?.completion_tokens ?? 0,
    },
    finish_reason: choice.finish_reason ?? 'stop',
  }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ChatCompletionResponse {
  choices?: Array<{
    index: number
    message: {
      role: string
      content: string | null
      reasoning_content?: string
      tool_calls?: Array<{
        id: string
        type: string
        function: { name: string; arguments: string }
      }>
    }
    finish_reason: string | null
  }>
  usage?: { prompt_tokens: number; completion_tokens: number; total_tokens: number }
}
