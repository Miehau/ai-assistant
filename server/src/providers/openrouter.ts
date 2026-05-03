import { logger } from '../lib/logger.js'
import type {
  LLMProvider,
  LLMRequest,
  LLMResponse,
  LLMStreamEvent,
  LLMToolCall,
  LLMToolDefinition,
  LLMAudioTranscriptionRequest,
  LLMAudioTranscriptionResponse,
} from './types.js'

const BASE_URL = 'https://openrouter.ai/api/v1/chat/completions'
const AUDIO_TRANSCRIPTIONS_URL = 'https://openrouter.ai/api/v1/audio/transcriptions'
const DEFAULT_MAX_TOKENS = 12_000

function buildTokenLimit(model: string, maxTokens?: number): Record<string, number> {
  return { max_tokens: maxTokens ?? DEFAULT_MAX_TOKENS }
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
 * /api/v1/chat/completions endpoint with real SSE streaming support.
 */
export class OpenRouterProvider implements LLMProvider {
  private apiKey: string

  constructor(apiKey: string) {
    this.apiKey = apiKey
  }

  async transcribeAudio(request: LLMAudioTranscriptionRequest): Promise<LLMAudioTranscriptionResponse> {
    const body = buildAudioTranscriptionRequestBody(request)
    preflightAudioTranscriptionRequestBody(body)

    const res = await fetch(AUDIO_TRANSCRIPTIONS_URL, {
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
      throw new Error(`OpenRouter transcription ${res.status}: ${err}`)
    }

    const json = await res.json() as OpenRouterTranscriptionResponse
    if (typeof json.text !== 'string') {
      throw new Error('OpenRouter transcription response missing text')
    }

    return {
      text: json.text,
      usage: json.usage,
    }
  }

  async generate(request: LLMRequest): Promise<LLMResponse> {
    const body = buildRequestBody(request)
    preflightRequestBody(body)

    logger.debug({ provider: 'openrouter', model: request.model, messages: body.messages }, 'LLM request messages')
    logger.debug('LLM request tools:\n%s', JSON.stringify(body.tools, null, 2))

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
    const body = {
      ...buildRequestBody(request),
      stream: true,
      stream_options: { include_usage: true },
    }
    preflightRequestBody(body)

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

    if (!res.body) {
      throw new Error('OpenRouter stream body is null')
    }

    let fullText = ''
    const toolCallBuffers: Record<number, { call_id: string; name: string; args: string }> = {}
    let finishReason = ''
    let completionTokens = 0
    let promptTokens = 0

    for await (const chunk of parseOpenRouterSSE(res.body)) {
      if (chunk.usage) {
        promptTokens = chunk.usage.prompt_tokens ?? promptTokens
        completionTokens = chunk.usage.completion_tokens ?? completionTokens
      }

      const choice = chunk.choices?.[0]
      if (!choice) continue

      if (choice.finish_reason) {
        finishReason = choice.finish_reason
      }

      const delta = choice.delta
      if (!delta) continue

      if (typeof delta.content === 'string' && delta.content.length > 0) {
        fullText += delta.content
        yield { type: 'text_delta', text: delta.content }
      }

      const reasoningChunk = delta.reasoning_content
      if (typeof reasoningChunk === 'string' && reasoningChunk.length > 0) {
        fullText += reasoningChunk
        yield { type: 'text_delta', text: reasoningChunk }
      }

      if (delta.tool_calls) {
        for (const tc of delta.tool_calls) {
          const idx = tc.index ?? 0

          if (tc.id) {
            const restoredName = restoreToolName(tc.function?.name ?? '')
            toolCallBuffers[idx] = {
              call_id: tc.id,
              name: restoredName,
              args: tc.function?.arguments ?? '',
            }
            yield {
              type: 'tool_call_delta',
              call_id: tc.id,
              name: restoredName || undefined,
              arguments_delta: tc.function?.arguments ?? '',
            }
            continue
          }

          const existing = toolCallBuffers[idx]
          if (!existing) continue

          if (tc.function?.name) {
            existing.name = restoreToolName(tc.function.name)
          }
          if (tc.function?.arguments) {
            existing.args += tc.function.arguments
            yield {
              type: 'tool_call_delta',
              call_id: existing.call_id,
              arguments_delta: tc.function.arguments,
            }
          }
        }
      }

      if (choice.finish_reason) {
        for (const buf of Object.values(toolCallBuffers)) {
          yield {
            type: 'tool_call_done',
            call_id: buf.call_id,
            name: buf.name,
            arguments: buf.args,
          }
        }
      }
    }

    if (fullText) {
      yield { type: 'text_done', text: fullText }
    }

    const toolCalls: LLMToolCall[] = Object.values(toolCallBuffers).map((buf) => ({
      call_id: buf.call_id,
      name: buf.name,
      arguments: buf.args ? JSON.parse(buf.args) : {},
    }))

    const response: LLMResponse = {
      content: fullText,
      tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
      companion_text: toolCalls.length > 0 && fullText ? fullText : undefined,
      usage: {
        input_tokens: promptTokens,
        output_tokens: completionTokens,
      },
      finish_reason: finishReason || 'stop',
    }

    logger.debug({ provider: 'openrouter', model: request.model, raw: response }, 'Raw LLM response (stream)')

    yield { type: 'done', response }
  }
}

async function* parseOpenRouterSSE(body: ReadableStream<Uint8Array>): AsyncIterable<ChatCompletionChunk> {
  const reader = body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const blocks = buffer.split(/\r?\n\r?\n/)
      buffer = blocks.pop() ?? ''

      for (const block of blocks) {
        const chunk = parseOpenRouterSSEBlock(block)
        if (!chunk) continue
        if (chunk === '[DONE]') return
        yield chunk
      }
    }

    if (buffer.trim()) {
      const chunk = parseOpenRouterSSEBlock(buffer)
      if (chunk && chunk !== '[DONE]') {
        yield chunk
      }
    }
  } finally {
    reader.releaseLock()
  }
}

function parseOpenRouterSSEBlock(block: string): ChatCompletionChunk | '[DONE]' | null {
  const lines = block.split(/\r?\n/)
  let data = ''

  for (const line of lines) {
    if (line.startsWith('data:')) {
      data += line.slice(5).trim()
    }
  }

  if (!data) return null
  if (data === '[DONE]') return '[DONE]'

  return JSON.parse(data) as ChatCompletionChunk
}

export function buildAudioTranscriptionRequestBody(request: LLMAudioTranscriptionRequest): Record<string, unknown> {
  return {
    model: request.model,
    input_audio: {
      data: request.input_audio.data,
      format: request.input_audio.format,
    },
  }
}

// ---------------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------------

export function buildRequestBody(request: LLMRequest): Record<string, unknown> {
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
    ? request.tools.map(mapToolDefinition)
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

function mapToolDefinition(tool: LLMToolDefinition): Record<string, unknown> {
  if (tool.name === 'web_search') {
    return { type: 'openrouter:web_search' }
  }
  return {
    type: 'function',
    function: {
      name: sanitizeToolName(tool.name),
      description: tool.description,
      parameters: tool.parameters,
    },
  }
}

export function preflightRequestBody(body: Record<string, unknown>): void {
  if (typeof body.model !== 'string' || !body.model) {
    throw new Error('Invalid OpenRouter request: model is required')
  }
  if (!Array.isArray(body.messages)) {
    throw new Error('Invalid OpenRouter request: messages must be an array')
  }

  const tools = body.tools
  if (tools === undefined) return
  if (!Array.isArray(tools)) {
    throw new Error('Invalid OpenRouter request: tools must be an array')
  }

  for (const [index, rawTool] of tools.entries()) {
    if (!rawTool || typeof rawTool !== 'object') {
      throw new Error(`Invalid OpenRouter request: tools[${index}] must be an object`)
    }
    const tool = rawTool as Record<string, unknown>
    if (tool.type === 'openrouter:web_search') continue
    if (tool.type === 'function' && isFunctionToolPayload(tool.function)) continue
    throw new Error(`Invalid OpenRouter request: unsupported tools[${index}] shape`)
  }
}

export function preflightAudioTranscriptionRequestBody(body: Record<string, unknown>): void {
  if (typeof body.model !== 'string' || !body.model) {
    throw new Error('Invalid OpenRouter transcription request: model is required')
  }

  const inputAudio = body.input_audio
  if (!inputAudio || typeof inputAudio !== 'object') {
    throw new Error('Invalid OpenRouter transcription request: input_audio is required')
  }

  const audio = inputAudio as Record<string, unknown>
  if (typeof audio.data !== 'string' || audio.data.length === 0) {
    throw new Error('Invalid OpenRouter transcription request: input_audio.data is required')
  }
  if (audio.data.startsWith('data:')) {
    throw new Error('Invalid OpenRouter transcription request: input_audio.data must be raw base64')
  }
  if (typeof audio.format !== 'string' || audio.format.length === 0) {
    throw new Error('Invalid OpenRouter transcription request: input_audio.format is required')
  }
}

function isFunctionToolPayload(value: unknown): boolean {
  if (!value || typeof value !== 'object') return false
  const fn = value as Record<string, unknown>
  return typeof fn.name === 'string' &&
    typeof fn.description === 'string' &&
    !!fn.parameters &&
    typeof fn.parameters === 'object'
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

interface ChatCompletionChunk {
  choices?: Array<{
    index: number
    delta?: {
      content?: string | null
      reasoning_content?: string
      tool_calls?: Array<{
        index?: number
        id?: string
        type?: string
        function?: {
          name?: string
          arguments?: string
        }
      }>
    }
    finish_reason?: string | null
  }>
  usage?: { prompt_tokens?: number; completion_tokens?: number; total_tokens?: number }
}

interface OpenRouterTranscriptionResponse {
  text?: string
  usage?: unknown
}
