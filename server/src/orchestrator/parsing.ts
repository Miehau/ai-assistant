import type { ControllerAction, ToolCallSpec } from './types.js'
import type { LLMToolCall } from '../providers/types.js'

const JSON_START_MARKER = '=====JSON_START====='
const JSON_END_MARKER = '=====JSON_END====='

// ---------------------------------------------------------------------------
// JSON Schema for controller structured output
// ---------------------------------------------------------------------------

export function controllerOutputSchema(): Record<string, unknown> {
  return {
    type: 'object',
    properties: {
      action: {
        type: 'string',
        enum: ['next_step', 'complete', 'guardrail_stop', 'ask_user'],
      },
      // next_step fields
      thinking: { type: ['string', 'object', 'null'] },
      step_type: { type: 'string' },
      tool: { type: 'string' },
      tools: {
        type: 'array',
        items: {
          type: 'object',
          properties: {
            tool: { type: 'string' },
            args: { type: 'object' },
            save: { type: 'boolean' },
          },
          required: ['tool', 'args'],
        },
      },
      args: { type: 'object' },
      message: { type: 'string' },
      question: { type: 'string' },
      context: { type: 'string' },
      save: { type: 'boolean' },
      // complete fields — message reused
      // guardrail_stop fields
      reason: { type: 'string' },
    },
    required: ['action'],
  }
}

// ---------------------------------------------------------------------------
// Extract JSON from text with markers or markdown fences
// ---------------------------------------------------------------------------

export function extractJson(text: string): string {
  // Try =====JSON_START===== markers first
  const startIdx = text.indexOf(JSON_START_MARKER)
  if (startIdx !== -1) {
    const afterStart = startIdx + JSON_START_MARKER.length
    const endIdx = text.indexOf(JSON_END_MARKER, afterStart)
    if (endIdx === -1) {
      throw new Error('Found JSON_START marker but no JSON_END marker')
    }
    return text.slice(afterStart, endIdx).trim()
  }

  // Try markdown ```json fences
  const jsonFenceMatch = text.match(/```json\s*\n([\s\S]*?)\n\s*```/)
  if (jsonFenceMatch) {
    return jsonFenceMatch[1].trim()
  }

  // Try bare ``` fences
  const bareFenceMatch = text.match(/```\s*\n([\s\S]*?)\n\s*```/)
  if (bareFenceMatch) {
    return bareFenceMatch[1].trim()
  }

  // Try to find a top-level JSON object
  const firstBrace = text.indexOf('{')
  const lastBrace = text.lastIndexOf('}')
  if (firstBrace !== -1 && lastBrace > firstBrace) {
    return text.slice(firstBrace, lastBrace + 1)
  }

  throw new Error('No JSON found in response text')
}

// ---------------------------------------------------------------------------
// Map native tool_calls from LLMResponse into a ControllerAction
// ---------------------------------------------------------------------------

export function mapToolCallsToAction(toolCalls: LLMToolCall[]): ControllerAction {
  if (toolCalls.length === 0) {
    throw new Error('Empty tool_calls array — cannot map to action')
  }

  if (toolCalls.length === 1) {
    const tc = toolCalls[0]
    return {
      action: 'next_step',
      tool: tc.name,
      args: tc.arguments,
    }
  }

  // Multiple tool calls → batch
  const specs: ToolCallSpec[] = toolCalls.map((tc) => ({
    tool: tc.name,
    args: tc.arguments,
  }))
  return {
    action: 'next_step',
    tools: specs,
  }
}

// ---------------------------------------------------------------------------
// Parse controller action from LLM response
// ---------------------------------------------------------------------------

export function parseControllerAction(response: unknown): ControllerAction {
  // If response is already an object (structured output or pre-parsed)
  if (response !== null && typeof response === 'object' && !Array.isArray(response)) {
    return validateAction(response as Record<string, unknown>)
  }

  // If response is a string, extract and parse JSON
  if (typeof response === 'string') {
    const jsonStr = extractJson(response)
    let parsed: unknown
    try {
      parsed = JSON.parse(jsonStr)
    } catch (err) {
      throw new Error(
        `Failed to parse controller JSON: ${err instanceof Error ? err.message : String(err)}`
      )
    }
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error('Controller response must be a JSON object')
    }
    return validateAction(parsed as Record<string, unknown>)
  }

  throw new Error(`Unexpected controller response type: ${typeof response}`)
}

// ---------------------------------------------------------------------------
// Validate parsed action
// ---------------------------------------------------------------------------

function validateAction(raw: Record<string, unknown>): ControllerAction {
  const action = raw.action as string | undefined
  if (!action) {
    throw new Error('Controller response missing required "action" field')
  }

  switch (action) {
    case 'next_step':
      return validateNextStep(raw)

    case 'complete':
      if (typeof raw.message !== 'string' || raw.message.length === 0) {
        throw new Error('"complete" action requires a non-empty "message" string')
      }
      return { action: 'complete', message: raw.message }

    case 'guardrail_stop':
      if (typeof raw.reason !== 'string' || raw.reason.length === 0) {
        throw new Error('"guardrail_stop" action requires a non-empty "reason" string')
      }
      return {
        action: 'guardrail_stop',
        reason: raw.reason,
        message: typeof raw.message === 'string' ? raw.message : undefined,
      }

    case 'ask_user':
      if (typeof raw.question !== 'string' || raw.question.length === 0) {
        throw new Error('"ask_user" action requires a non-empty "question" string')
      }
      return {
        action: 'ask_user',
        question: raw.question,
        context: typeof raw.context === 'string' ? raw.context : undefined,
      }

    default:
      throw new Error(`Unknown controller action: "${action}"`)
  }
}

function validateNextStep(raw: Record<string, unknown>): ControllerAction & { action: 'next_step' } {
  const result: ControllerAction & { action: 'next_step' } = { action: 'next_step' }

  if (raw.thinking !== undefined) result.thinking = raw.thinking
  if (typeof raw.step_type === 'string') result.step_type = raw.step_type
  if (typeof raw.tool === 'string') result.tool = raw.tool
  if (raw.args && typeof raw.args === 'object') result.args = raw.args as Record<string, unknown>
  if (typeof raw.message === 'string') result.message = raw.message
  if (typeof raw.question === 'string') result.question = raw.question
  if (typeof raw.context === 'string') result.context = raw.context
  if (typeof raw.save === 'boolean') result.save = raw.save

  if (Array.isArray(raw.tools)) {
    result.tools = (raw.tools as Record<string, unknown>[]).map((t) => {
      if (typeof t.tool !== 'string') throw new Error('Each tool in "tools" must have a "tool" name')
      return {
        tool: t.tool,
        args: (t.args && typeof t.args === 'object' ? t.args : {}) as Record<string, unknown>,
        save: typeof t.save === 'boolean' ? t.save : undefined,
      }
    })
  }

  // Validate that at least one meaningful field is present
  const inferred = inferStepType(result)
  if (!inferred) {
    throw new Error(
      '"next_step" action must include at least one of: tool, tools, message, or question'
    )
  }

  return result
}

// ---------------------------------------------------------------------------
// Infer step type from field presence
// ---------------------------------------------------------------------------

export function inferStepType(
  action: ControllerAction
): 'tool' | 'tool_batch' | 'respond' | 'ask_user' | null {
  if (action.action !== 'next_step') return null

  if ('tools' in action && Array.isArray(action.tools) && action.tools.length > 0) {
    return 'tool_batch'
  }
  if ('tool' in action && typeof action.tool === 'string') {
    return 'tool'
  }
  if ('message' in action && typeof action.message === 'string') {
    return 'respond'
  }
  if ('question' in action && typeof action.question === 'string') {
    return 'ask_user'
  }
  return null
}
