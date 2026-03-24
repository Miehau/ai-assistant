import type { LLMMessage, LLMContentBlock } from '../providers/types.js'
import type { Item } from '../domain/types.js'

// ---------------------------------------------------------------------------
// Base prompt — uses JSON markers for providers without native tool calling
// ---------------------------------------------------------------------------

export const CONTROLLER_PROMPT_BASE = `You are a controller that decides the next action for an AI agent.

Your job is to analyze the conversation history and decide what to do next.
You have access to a set of tools that you can invoke to accomplish the user's task.

Available actions:
- next_step: Execute a tool or respond to the user
  - To call a single tool: set "tool" and "args"
  - To call multiple tools: set "tools" array with [{tool, args}]
  - To respond to the user: set "message"
  - To ask for clarification: set "question"
- complete: Finish the conversation with a final "message"
- guardrail_stop: Stop if the request is inappropriate, provide "reason"
- ask_user: Ask the user a clarifying question, provide "question"

Always respond with JSON between =====JSON_START===== and =====JSON_END===== markers.

Example:
=====JSON_START=====
{"action": "next_step", "tool": "web_search", "args": {"query": "example"}}
=====JSON_END=====`

// ---------------------------------------------------------------------------
// Anthropic prompt — uses native function calling (no JSON markers)
// ---------------------------------------------------------------------------

export const CONTROLLER_PROMPT_ANTHROPIC = `You are a controller that decides the next action for an AI agent.

Your job is to analyze the conversation history and decide what to do next.
You have access to tools that you can invoke using function calling.

Call the appropriate tool to take the next step, or respond with text to:
- Complete the task (provide a final message to the user)
- Ask the user a clarifying question
- Stop if the request is inappropriate

Be precise and efficient. Execute tools when needed, respond when the task is done.`

// ---------------------------------------------------------------------------
// OpenAI prompt — uses native function calling (no JSON markers)
// ---------------------------------------------------------------------------

export const CONTROLLER_PROMPT_OPENAI = `You are a controller that decides the next action for an AI agent.

Your job is to analyze the conversation history and decide what to do next.
You have access to tools that you can invoke using function calling.

Call the appropriate tool to take the next step, or respond with text to:
- Complete the task (provide a final message to the user)
- Ask the user a clarifying question
- Stop if the request is inappropriate

Be precise and efficient. Execute tools when needed, respond when the task is done.`

// ---------------------------------------------------------------------------
// Build tool list string for inclusion in system prompt
// ---------------------------------------------------------------------------

function buildToolListString(tools: Array<{ name: string; description: string; parameters: Record<string, unknown> }>): string {
  if (tools.length === 0) return 'No tools available.'

  const lines = tools.map((t) => {
    const params = JSON.stringify(t.parameters, null, 2)
    return `### ${t.name}\n${t.description}\nParameters: ${params}`
  })
  return lines.join('\n\n')
}

// ---------------------------------------------------------------------------
// Convert Item history to LLMMessages
// ---------------------------------------------------------------------------

function itemToMessages(items: Item[]): LLMMessage[] {
  const messages: LLMMessage[] = []

  // Build a set of call IDs that have a corresponding output, so we can detect
  // orphaned function_call items (e.g. interrupted execution) and inject a
  // synthetic error response to satisfy the OpenAI tool_call_id contract.
  const outputCallIds = new Set(
    items.filter((i) => i.type === 'function_call_output').map((i) => i.callId),
  )

  for (let i = 0; i < items.length; i++) {
    const item = items[i]

    switch (item.type) {
      case 'message': {
        const role = item.role ?? 'user'
        if (role === 'system') continue // system messages handled separately
        messages.push({
          role: role as 'user' | 'assistant',
          content: item.content ?? '',
        })
        break
      }

      case 'function_call': {
        // Merge consecutive function_call items into a single assistant message
        // with multiple tool_calls. This is critical for OpenAI compatibility —
        // an assistant message with tool_calls must be followed by tool messages
        // for ALL call IDs before any other message type.
        const toolCalls = [{
          call_id: item.callId ?? '',
          name: item.name ?? '',
          arguments: item.arguments ? JSON.parse(item.arguments) : {},
        }]

        // Consume any consecutive function_call items that follow
        while (i + 1 < items.length && items[i + 1].type === 'function_call') {
          i++
          const next = items[i]
          toolCalls.push({
            call_id: next.callId ?? '',
            name: next.name ?? '',
            arguments: next.arguments ? JSON.parse(next.arguments) : {},
          })
        }

        messages.push({
          role: 'assistant',
          content: '',
          tool_calls: toolCalls,
        })

        // Inject synthetic error responses for any orphaned tool calls (no output in history)
        for (const tc of toolCalls) {
          if (tc.call_id && !outputCallIds.has(tc.call_id)) {
            messages.push({
              role: 'tool',
              content: '[Tool execution was interrupted]',
              tool_call_id: tc.call_id,
            })
          }
        }
        break
      }

      case 'function_call_output': {
        // Tool result — role=tool with tool_call_id
        // If the tool returned content blocks (e.g. images), pass them as an array
        // so provider mappers can render them correctly (Anthropic: tool_result content array,
        // OpenAI: split into tool result + synthetic user message).
        if (item.contentBlocks && item.contentBlocks.length > 0) {
          messages.push({
            role: 'tool',
            content: item.contentBlocks as LLMContentBlock[],
            tool_call_id: item.callId ?? undefined,
          })
        } else {
          messages.push({
            role: 'tool',
            content: item.output ?? item.content ?? '',
            tool_call_id: item.callId ?? undefined,
          })
        }
        break
      }

      case 'reasoning': {
        // Skip reasoning items — internal only
        break
      }
    }
  }

  return messages
}

// ---------------------------------------------------------------------------
// Build the full controller message array
// ---------------------------------------------------------------------------

export interface BuildMessagesConfig {
  useNativeFunctionCalling: boolean
  agentTask: string
  customSystemPrompt?: string
}

export function buildControllerMessages(
  systemPrompt: string,
  toolList: string,
  history: Item[],
  config: BuildMessagesConfig,
): LLMMessage[] {
  const messages: LLMMessage[] = []

  // System message: controller prompt + task + available tools
  let systemContent = systemPrompt

  if (config.agentTask) {
    systemContent += `\n\n## Current Task\n${config.agentTask}`
  }

  if (!config.useNativeFunctionCalling && toolList) {
    systemContent += `\n\n## Available Tools\n${toolList}`
  }

  if (config.customSystemPrompt) {
    systemContent += `\n\n## Additional Instructions\n${config.customSystemPrompt}`
  }

  messages.push({ role: 'system', content: systemContent })

  // Append conversation history
  const historyMessages = itemToMessages(history)
  messages.push(...historyMessages)

  return messages
}

export { buildToolListString }
