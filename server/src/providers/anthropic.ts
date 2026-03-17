import Anthropic from '@anthropic-ai/sdk'
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

/** Replace dots with __ so tool names satisfy Anthropic's ^[a-zA-Z0-9_-]{1,128}$ requirement. */
function sanitizeToolName(name: string): string {
  return name.replace(/\./g, '__')
}

/** Reverse sanitizeToolName — restores original dot-separated names. */
function restoreToolName(name: string): string {
  return name.replace(/__/g, '.')
}

/**
 * Recursively strips oneOf, anyOf, allOf from a JSON schema object.
 * Anthropic's API does not support these keywords.
 */
function stripUnsupportedSchemaKeywords(schema: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(schema)) {
    if (key === 'oneOf' || key === 'anyOf' || key === 'allOf') {
      continue
    }
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      result[key] = stripUnsupportedSchemaKeywords(value as Record<string, unknown>)
    } else if (Array.isArray(value)) {
      result[key] = value.map((item) =>
        item && typeof item === 'object' && !Array.isArray(item)
          ? stripUnsupportedSchemaKeywords(item as Record<string, unknown>)
          : item,
      )
    } else {
      result[key] = value
    }
  }
  return result
}

function logCacheStats(
  label: string,
  input: number,
  cacheRead: number,
  cacheWrite: number,
): void {
  const total = input + cacheRead + cacheWrite
  if (total === 0) return
  const hitPct = ((cacheRead / total) * 100).toFixed(1)
  const writePct = ((cacheWrite / total) * 100).toFixed(1)
  const freshPct = ((input / total) * 100).toFixed(1)
  console.log(
    `[anthropic] ${label} — cache ${hitPct}% hit, ${writePct}% write, ${freshPct}% fresh` +
    ` (read=${cacheRead}, write=${cacheWrite}, fresh=${input}, total=${total})`,
  )
}

function mapMessagesToAnthropic(
  messages: LLMMessage[],
): { system: Anthropic.TextBlockParam[] | undefined; messages: Anthropic.MessageParam[] } {
  let systemText: string | undefined
  const mapped: Anthropic.MessageParam[] = []

  for (const msg of messages) {
    if (msg.role === 'system') {
      systemText = typeof msg.content === 'string' ? msg.content : ''
      continue
    }

    if (msg.role === 'tool') {
      if (Array.isArray(msg.content)) {
        // Multimodal tool result — map each block into Anthropic's tool_result content array
        const contentBlocks: Array<Anthropic.TextBlockParam | Anthropic.ImageBlockParam> = []
        for (const block of msg.content as LLMContentBlock[]) {
          if (block.type === 'text' && block.text) {
            contentBlocks.push({ type: 'text', text: block.text })
          } else if (block.type === 'image' && block.data) {
            contentBlocks.push({
              type: 'image',
              source: {
                type: 'base64',
                media_type: (block.media_type ?? 'image/png') as 'image/png' | 'image/jpeg' | 'image/gif' | 'image/webp',
                data: block.data,
              },
            })
          }
        }
        mapped.push({
          role: 'user',
          content: [{ type: 'tool_result', tool_use_id: msg.tool_call_id!, content: contentBlocks }],
        })
      } else {
        mapped.push({
          role: 'user',
          content: [
            {
              type: 'tool_result',
              tool_use_id: msg.tool_call_id!,
              content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content),
            },
          ],
        })
      }
      continue
    }

    if (msg.role === 'assistant') {
      const blocks: Anthropic.ContentBlockParam[] = []

      if (typeof msg.content === 'string' && msg.content) {
        blocks.push({ type: 'text', text: msg.content })
      } else if (Array.isArray(msg.content)) {
        for (const block of msg.content as LLMContentBlock[]) {
          if (block.type === 'text' && block.text) {
            blocks.push({ type: 'text', text: block.text })
          } else if (block.type === 'tool_use') {
            blocks.push({
              type: 'tool_use',
              id: block.tool_use_id!,
              name: sanitizeToolName(block.name!),
              input: block.input ?? {},
            })
          }
        }
      }

      if (msg.tool_calls?.length) {
        for (const tc of msg.tool_calls) {
          blocks.push({
            type: 'tool_use',
            id: tc.call_id,
            name: sanitizeToolName(tc.name),
            input: tc.arguments,
          })
        }
      }

      if (blocks.length > 0) {
        mapped.push({ role: 'assistant', content: blocks })
      }
      continue
    }

    // user message
    if (typeof msg.content === 'string') {
      mapped.push({ role: 'user', content: msg.content })
    } else if (Array.isArray(msg.content)) {
      const blocks: Anthropic.ContentBlockParam[] = []
      for (const block of msg.content as LLMContentBlock[]) {
        if (block.type === 'text' && block.text) {
          blocks.push({ type: 'text', text: block.text })
        } else if (block.type === 'image' && block.data) {
          blocks.push({
            type: 'image',
            source: {
              type: 'base64',
              media_type: (block.media_type ?? 'image/png') as 'image/png' | 'image/jpeg' | 'image/gif' | 'image/webp',
              data: block.data,
            },
          })
        } else if (block.type === 'tool_result') {
          blocks.push({
            type: 'tool_result',
            tool_use_id: block.tool_use_id!,
            content: block.content ?? '',
          })
        }
      }
      mapped.push({ role: 'user', content: blocks })
    }
  }

  const system: Anthropic.TextBlockParam[] | undefined = systemText != null
    ? [{ type: 'text', text: systemText }]
    : undefined

  return { system, messages: mapped }
}

function mapToolsToAnthropic(tools: LLMToolDefinition[]): Anthropic.Tool[] {
  return tools.map((tool) => ({
    name: sanitizeToolName(tool.name),
    description: tool.description,
    input_schema: stripUnsupportedSchemaKeywords(tool.parameters) as Anthropic.Tool.InputSchema,
  }))
}

export class AnthropicProvider implements LLMProvider {
  private client: Anthropic

  constructor(apiKey: string) {
    this.client = new Anthropic({ apiKey })
  }

  async generate(request: LLMRequest): Promise<LLMResponse> {
    const { system, messages } = mapMessagesToAnthropic(request.messages)

    let tools = request.tools ? mapToolsToAnthropic(request.tools) : undefined
    let toolChoice: Anthropic.MessageCreateParams['tool_choice'] = undefined

    // For structured output, add a synthetic tool and force its use
    if (request.structured_output) {
      const structuredTool: Anthropic.Tool = {
        name: '_structured_output',
        description: 'Return the structured response in the required format.',
        input_schema: stripUnsupportedSchemaKeywords(
          request.structured_output,
        ) as Anthropic.Tool.InputSchema,
      }
      tools = [...(tools ?? []), structuredTool]
      toolChoice = { type: 'tool', name: '_structured_output' }
    }

    const params: Anthropic.MessageCreateParams = {
      model: request.model,
      max_tokens: request.max_tokens ?? 4096,
      messages,
      cache_control: { type: 'ephemeral' },
      ...(system && { system }),
      ...(tools && { tools }),
      ...(toolChoice && { tool_choice: toolChoice }),
      ...(request.temperature !== undefined && { temperature: request.temperature }),
    }

    const response = await this.client.messages.create(params)

    logCacheStats(
      'generate',
      response.usage.input_tokens,
      response.usage.cache_read_input_tokens ?? 0,
      response.usage.cache_creation_input_tokens ?? 0,
    )

    return mapAnthropicResponse(response, !!request.structured_output)
  }

  async *stream(request: LLMRequest): AsyncIterable<LLMStreamEvent> {
    const { system, messages } = mapMessagesToAnthropic(request.messages)

    let tools = request.tools ? mapToolsToAnthropic(request.tools) : undefined
    let toolChoice: Anthropic.MessageCreateParams['tool_choice'] = undefined

    if (request.structured_output) {
      const structuredTool: Anthropic.Tool = {
        name: '_structured_output',
        description: 'Return the structured response in the required format.',
        input_schema: stripUnsupportedSchemaKeywords(
          request.structured_output,
        ) as Anthropic.Tool.InputSchema,
      }
      tools = [...(tools ?? []), structuredTool]
      toolChoice = { type: 'tool', name: '_structured_output' }
    }

    const params: Anthropic.MessageCreateParams = {
      model: request.model,
      max_tokens: request.max_tokens ?? 4096,
      messages,
      cache_control: { type: 'ephemeral' },
      ...(system && { system }),
      ...(tools && { tools }),
      ...(toolChoice && { tool_choice: toolChoice }),
      ...(request.temperature !== undefined && { temperature: request.temperature }),
      stream: true,
    }

    const stream = await this.client.messages.create(params)

    // Track state for building the final response
    let fullText = ''
    const toolCallArgs: Record<string, { id: string; name: string; args: string }> = {}
    let inputTokens = 0
    let outputTokens = 0
    let cacheReadTokens = 0
    let cacheCreationTokens = 0
    let stopReason = ''

    for await (const event of stream as AsyncIterable<Anthropic.MessageStreamEvent>) {
      switch (event.type) {
        case 'message_start':
          if (event.message.usage) {
            inputTokens = event.message.usage.input_tokens
            cacheReadTokens = event.message.usage.cache_read_input_tokens ?? 0
            cacheCreationTokens = event.message.usage.cache_creation_input_tokens ?? 0
            logCacheStats('stream', inputTokens, cacheReadTokens, cacheCreationTokens)
          }
          break

        case 'message_delta':
          if (event.usage) {
            outputTokens = event.usage.output_tokens
          }
          stopReason = event.delta.stop_reason ?? stopReason
          break

        case 'content_block_start':
          if (event.content_block.type === 'tool_use') {
            const block = event.content_block
            const restoredName = restoreToolName(block.name)
            toolCallArgs[event.index] = { id: block.id, name: restoredName, args: '' }
            yield {
              type: 'tool_call_delta',
              call_id: block.id,
              name: restoredName,
              arguments_delta: '',
            }
          }
          break

        case 'content_block_delta':
          if (event.delta.type === 'text_delta') {
            fullText += event.delta.text
            yield { type: 'text_delta', text: event.delta.text }
          } else if (event.delta.type === 'input_json_delta') {
            const tracked = toolCallArgs[event.index]
            if (tracked) {
              tracked.args += event.delta.partial_json
              yield {
                type: 'tool_call_delta',
                call_id: tracked.id,
                arguments_delta: event.delta.partial_json,
              }
            }
          }
          break

        case 'content_block_stop':
          if (toolCallArgs[event.index]) {
            const tracked = toolCallArgs[event.index]
            yield {
              type: 'tool_call_done',
              call_id: tracked.id,
              name: tracked.name,
              arguments: tracked.args,
            }
          }
          break

        case 'message_stop': {
          // Build the final response by re-parsing collected data
          const toolCalls: LLMToolCall[] = Object.values(toolCallArgs).map((tc) => ({
            call_id: tc.id,
            name: tc.name,
            arguments: tc.args ? JSON.parse(tc.args) : {},
          }))

          const response: LLMResponse = {
            content: request.structured_output && toolCalls.length > 0
              ? toolCalls.find((tc) => tc.name === '_structured_output')?.arguments ?? fullText
              : fullText,
            tool_calls: toolCalls.filter((tc) => tc.name !== '_structured_output').length > 0
              ? toolCalls.filter((tc) => tc.name !== '_structured_output')
              : undefined,
            companion_text: toolCalls.length > 0 && fullText ? fullText : undefined,
            usage: {
              input_tokens: inputTokens,
              output_tokens: outputTokens,
              ...(cacheReadTokens > 0 && { cache_read_input_tokens: cacheReadTokens }),
              ...(cacheCreationTokens > 0 && { cache_creation_input_tokens: cacheCreationTokens }),
            },
            finish_reason: stopReason || 'end_turn',
          }

          if (fullText) {
            yield { type: 'text_done', text: fullText }
          }
          yield { type: 'done', response }
          break
        }
      }
    }
  }
}

function mapAnthropicResponse(
  response: Anthropic.Message,
  isStructuredOutput: boolean,
): LLMResponse {
  let text = ''
  const toolCalls: LLMToolCall[] = []

  for (const block of response.content) {
    if (block.type === 'text') {
      text += block.text
    } else if (block.type === 'tool_use') {
      toolCalls.push({
        call_id: block.id,
        name: restoreToolName(block.name),
        arguments: block.input as Record<string, unknown>,
      })
    }
  }

  const usage = {
    input_tokens: response.usage.input_tokens,
    output_tokens: response.usage.output_tokens,
    ...((response.usage.cache_read_input_tokens ?? 0) > 0 && { cache_read_input_tokens: response.usage.cache_read_input_tokens ?? undefined }),
    ...((response.usage.cache_creation_input_tokens ?? 0) > 0 && { cache_creation_input_tokens: response.usage.cache_creation_input_tokens ?? undefined }),
  }

  // For structured output, return the _structured_output tool's input as content
  if (isStructuredOutput) {
    const structuredCall = toolCalls.find((tc) => tc.name === '_structured_output')
    if (structuredCall) {
      return {
        content: structuredCall.arguments,
        companion_text: text || undefined,
        tool_calls: toolCalls.filter((tc) => tc.name !== '_structured_output').length > 0
          ? toolCalls.filter((tc) => tc.name !== '_structured_output')
          : undefined,
        usage,
        finish_reason: response.stop_reason ?? 'end_turn',
      }
    }
  }

  return {
    content: text,
    companion_text: toolCalls.length > 0 && text ? text : undefined,
    tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
    usage,
    finish_reason: response.stop_reason ?? 'end_turn',
  }
}
