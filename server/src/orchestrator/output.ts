import type { ToolResult } from '../tools/types.js'

const MAX_OUTPUT_BYTES = 32 * 1024 // 32 KB

// ---------------------------------------------------------------------------
// Format a single tool result for LLM context
// ---------------------------------------------------------------------------

export function buildToolResultMessage(
  toolName: string,
  callId: string,
  result: ToolResult,
): string {
  if (!result.ok) {
    return `[Tool Error: ${toolName}] ${result.error ?? 'Unknown error'}`
  }

  let body: string
  if (result.output === undefined || result.output === null) {
    body = '(no output)'
  } else if (typeof result.output === 'string') {
    body = result.output
  } else {
    body = JSON.stringify(result.output, null, 2)
  }

  body = truncateIfNeeded(body, toolName)
  return body
}

// ---------------------------------------------------------------------------
// Format batch results
// ---------------------------------------------------------------------------

export function buildBatchResultMessage(
  results: Array<{ callId: string; name: string; result: ToolResult }>,
): string {
  const parts = results.map((r) => {
    const header = `### ${r.name} (${r.callId})`
    const body = buildToolResultMessage(r.name, r.callId, r.result)
    return `${header}\n${body}`
  })

  let combined = parts.join('\n\n')
  combined = truncateIfNeeded(combined, 'batch')
  return combined
}

// ---------------------------------------------------------------------------
// Truncation helper
// ---------------------------------------------------------------------------

function truncateIfNeeded(text: string, toolName: string): string {
  const byteLength = Buffer.byteLength(text, 'utf-8')
  if (byteLength <= MAX_OUTPUT_BYTES) {
    return text
  }

  // Truncate to roughly MAX_OUTPUT_BYTES (conservative: char-based cut)
  // Walk backwards to find a safe cut point
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
