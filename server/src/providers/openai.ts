import OpenAI from 'openai'
import type {
  LLMProvider,
  LLMRequest,
  LLMResponse,
  LLMStreamEvent,
  LLMMessage,
  LLMToolDefinition,
  LLMToolCall,
  LLMContentBlock,
} from './types.js'

/**
 * GPT-5 family models require `max_completion_tokens` instead of `max_tokens`.
 * Sending `max_tokens` to GPT-5 causes a 400 error; omitting both causes 0 output tokens.
 */
const DEFAULT_MAX_COMPLETION_TOKENS = 16384

function needsMaxCompletionTokens(model: string): boolean {
  const m = model.toLowerCase()
  return m.includes('gpt-5') || m.includes('o3') || m.includes('o4')
}

function buildTokenLimit(model: string, maxTokens?: number): Record<string, number> {
  if (needsMaxCompletionTokens(model)) {
    return { max_completion_tokens: maxTokens ?? DEFAULT_MAX_COMPLETION_TOKENS }
  }
  if (maxTokens !== undefined) {
    return { max_tokens: maxTokens }
  }
  return {}
}

function mapMessagesToOpenAI(messages: LLMMessage[]): OpenAI.ChatCompletionMessageParam[] {
  const mapped: OpenAI.ChatCompletionMessageParam[] = []

  for (const msg of messages) {
    if (msg.role === 'system') {
      mapped.push({
        role: 'system',
        content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content),
      })
      continue
    }

    if (msg.role === 'tool') {
      if (Array.isArray(msg.content)) {
        // OpenAI tool role only accepts string content — extract text blocks for the tool result,
        // then inject a synthetic user message with any image blocks that follow.
        const textParts = (msg.content as LLMContentBlock[]).filter((b) => b.type === 'text')
        const imageParts = (msg.content as LLMContentBlock[]).filter((b) => b.type === 'image' && b.data)
        const textContent = textParts.map((b) => b.text ?? '').join('\n') || '(tool result)'
        mapped.push({
          role: 'tool',
          tool_call_id: msg.tool_call_id!,
          content: textContent,
        })
        if (imageParts.length > 0) {
          mapped.push({
            role: 'user',
            content: imageParts.map((b) => ({
              type: 'image_url' as const,
              image_url: { url: `data:${b.media_type ?? 'image/png'};base64,${b.data}` },
            })),
          } as OpenAI.ChatCompletionUserMessageParam)
        }
      } else {
        mapped.push({
          role: 'tool',
          tool_call_id: msg.tool_call_id!,
          content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content),
        })
      }
      continue
    }

    if (msg.role === 'assistant') {
      const toolCalls = msg.tool_calls?.map((tc) => ({
          id: tc.call_id,
          type: 'function' as const,
          function: {
            name: tc.name,
            arguments: JSON.stringify(tc.arguments),
          },
        }))

      const content = typeof msg.content === 'string' ? msg.content : null

      mapped.push({
        role: 'assistant',
        content,
        ...(toolCalls?.length && { tool_calls: toolCalls }),
      } as OpenAI.ChatCompletionAssistantMessageParam)
      continue
    }

    // user message
    if (typeof msg.content === 'string') {
      mapped.push({ role: 'user', content: msg.content })
    } else if (Array.isArray(msg.content)) {
      const parts: OpenAI.ChatCompletionContentPart[] = []
      for (const block of msg.content as LLMContentBlock[]) {
        if (block.type === 'text' && block.text) {
          parts.push({ type: 'text', text: block.text })
        } else if (block.type === 'image' && block.data) {
          parts.push({
            type: 'image_url',
            image_url: {
              url: `data:${block.media_type ?? 'image/png'};base64,${block.data}`,
            },
          })
        }
      }
      mapped.push({ role: 'user', content: parts })
    }
  }

  return mapped
}

function mapToolsToOpenAI(tools: LLMToolDefinition[]): OpenAI.ChatCompletionTool[] {
  return tools.map((tool) => ({
    type: 'function',
    function: {
      name: tool.name,
      description: tool.description,
      parameters: tool.parameters,
    },
  }))
}

export class OpenAIProvider implements LLMProvider {
  protected client: OpenAI

  constructor(apiKey: string, baseURL?: string) {
    this.client = new OpenAI({
      apiKey,
      ...(baseURL && { baseURL }),
    })
  }

  async generate(request: LLMRequest): Promise<LLMResponse> {
    const messages = mapMessagesToOpenAI(request.messages)
    const tools = request.tools?.length ? mapToolsToOpenAI(request.tools) : undefined

    const params: OpenAI.ChatCompletionCreateParams = {
      model: request.model,
      messages,
      ...(tools && { tools }),
      ...(request.temperature !== undefined && { temperature: request.temperature }),
      ...buildTokenLimit(request.model, request.max_tokens),
    }

    // Structured output via json_schema response format
    if (request.structured_output) {
      params.response_format = {
        type: 'json_schema',
        json_schema: {
          name: 'response',
          schema: request.structured_output,
          strict: true,
        },
      }
    }

    const response = await this.client.chat.completions.create(params)

    return mapOpenAIResponse(response)
  }

  async *stream(request: LLMRequest): AsyncIterable<LLMStreamEvent> {
    const messages = mapMessagesToOpenAI(request.messages)
    const tools = request.tools?.length ? mapToolsToOpenAI(request.tools) : undefined

    const params: OpenAI.ChatCompletionCreateParams = {
      model: request.model,
      messages,
      stream: true,
      ...(tools && { tools }),
      ...(request.temperature !== undefined && { temperature: request.temperature }),
      ...buildTokenLimit(request.model, request.max_tokens),
    }

    if (request.structured_output) {
      params.response_format = {
        type: 'json_schema',
        json_schema: {
          name: 'response',
          schema: request.structured_output,
          strict: true,
        },
      }
    }

    const stream = await this.client.chat.completions.create(params)

    let fullText = ''
    const toolCallBuffers: Record<number, { call_id: string; name: string; args: string }> = {}
    let finishReason = ''
    let completionTokens = 0
    let promptTokens = 0

    for await (const chunk of stream as AsyncIterable<OpenAI.ChatCompletionChunk>) {
      // Track usage if provided (some providers include it in chunks)
      if (chunk.usage) {
        promptTokens = chunk.usage.prompt_tokens
        completionTokens = chunk.usage.completion_tokens
      }

      const choice = chunk.choices?.[0]
      if (!choice) continue

      if (choice.finish_reason) {
        finishReason = choice.finish_reason
      }

      const delta = choice.delta

      // Text content
      if (delta?.content) {
        fullText += delta.content
        yield { type: 'text_delta', text: delta.content }
      }

      // Reasoning models (GPT-5, o-series) may stream thinking via reasoning_content
      // which isn't in the SDK types yet — access it from the raw object.
      const reasoningChunk = (delta as Record<string, unknown>)?.reasoning_content as string | undefined
      if (reasoningChunk) {
        fullText += reasoningChunk
        yield { type: 'text_delta', text: reasoningChunk }
      }

      // Tool call deltas
      if (delta?.tool_calls) {
        for (const tc of delta.tool_calls) {
          const idx = tc.index

          if (tc.id) {
            // New tool call starting
            toolCallBuffers[idx] = {
              call_id: tc.id,
              name: tc.function?.name ?? '',
              args: tc.function?.arguments ?? '',
            }
            yield {
              type: 'tool_call_delta',
              call_id: tc.id,
              name: tc.function?.name,
              arguments_delta: tc.function?.arguments ?? '',
            }
          } else if (toolCallBuffers[idx]) {
            // Continuation of existing tool call
            const buf = toolCallBuffers[idx]
            if (tc.function?.name) buf.name = tc.function.name
            if (tc.function?.arguments) {
              buf.args += tc.function.arguments
              yield {
                type: 'tool_call_delta',
                call_id: buf.call_id,
                arguments_delta: tc.function.arguments,
              }
            }
          }
        }
      }

      // On finish, emit done events for tool calls
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

    // Emit final events
    if (fullText) {
      yield { type: 'text_done', text: fullText }
    }

    const toolCalls: LLMToolCall[] = Object.values(toolCallBuffers).map((buf) => ({
      call_id: buf.call_id,
      name: buf.name,
      arguments: buf.args ? JSON.parse(buf.args) : {},
    }))

    yield {
      type: 'done',
      response: {
        content: fullText,
        tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
        companion_text: toolCalls.length > 0 && fullText ? fullText : undefined,
        usage: { input_tokens: promptTokens, output_tokens: completionTokens },
        finish_reason: finishReason || 'stop',
      },
    }
  }
}

function mapOpenAIResponse(response: OpenAI.ChatCompletion): LLMResponse {
  const choice = response.choices[0]

  if (!choice) {
    return {
      content: '',
      usage: {
        input_tokens: response.usage?.prompt_tokens ?? 0,
        output_tokens: response.usage?.completion_tokens ?? 0,
      },
      finish_reason: 'stop',
    }
  }

  // Reasoning models (GPT-5, o-series) may put output in reasoning_content
  const rawMessage = choice.message as unknown as Record<string, unknown>
  const text = choice.message.content
    ?? (rawMessage.reasoning_content as string | undefined)
    ?? ''

  const toolCalls: LLMToolCall[] = (choice.message.tool_calls ?? []).map((tc) => ({
    call_id: tc.id,
    name: tc.function.name,
    arguments: JSON.parse(tc.function.arguments),
  }))

  return {
    content: text,
    companion_text: toolCalls.length > 0 && text ? text : undefined,
    tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
    usage: {
      input_tokens: response.usage?.prompt_tokens ?? 0,
      output_tokens: response.usage?.completion_tokens ?? 0,
    },
    finish_reason: choice.finish_reason ?? 'stop',
  }
}
