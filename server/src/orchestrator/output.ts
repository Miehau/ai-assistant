import type { ToolResult } from '../tools/types.js'

const MAX_OUTPUT_BYTES = 32 * 1024 // 32 KB

// ---------------------------------------------------------------------------
// Format a tool result for storage and LLM context
// ---------------------------------------------------------------------------

export function formatToolOutput(result: ToolResult | ({ call_id: string } & ToolResult)): string {
  if (!result.ok) {
    return result.error ?? 'Unknown error'
  }
  if (result.output === undefined || result.output === null) {
    return '(no output)'
  }

  let body: string
  if (typeof result.output === 'string') {
    body = result.output
  } else {
    body = JSON.stringify(result.output)
  }

  return truncateIfNeeded(body)
}

// ---------------------------------------------------------------------------
// Truncation helper
// ---------------------------------------------------------------------------

function truncateIfNeeded(text: string): string {
  const byteLength = Buffer.byteLength(text, 'utf-8')
  if (byteLength <= MAX_OUTPUT_BYTES) {
    return text
  }

  // Truncate to roughly MAX_OUTPUT_BYTES (conservative: char-based cut)
  let cutPoint = text.length
  let currentBytes = byteLength
  while (currentBytes > MAX_OUTPUT_BYTES - 200 && cutPoint > 0) {
    cutPoint -= Math.max(1, Math.floor((currentBytes - MAX_OUTPUT_BYTES + 200) / 4))
    currentBytes = Buffer.byteLength(text.slice(0, cutPoint), 'utf-8')
  }

  const truncated = text.slice(0, Math.max(0, cutPoint))
  return (
    truncated +
    `\n\n[Output truncated — ${byteLength} bytes total. Use tool_outputs.extract with a JSONPath query to access specific data.]`
  )
}
