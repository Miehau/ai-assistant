// ---------------------------------------------------------------------------
// Tool argument hydration
// ---------------------------------------------------------------------------
// Automatically fills in missing arguments for tool_outputs.* tools based on
// the most recent tool output id, so the LLM doesn't have to repeat IDs.
// ---------------------------------------------------------------------------

const TOOL_OUTPUT_PREFIX = 'tool_outputs.'

export function hydrateToolArgs(
  toolName: string,
  args: Record<string, unknown>,
  lastOutputId?: string,
): Record<string, unknown> {
  if (!toolName.startsWith(TOOL_OUTPUT_PREFIX)) {
    return args
  }

  const hydrated = { ...args }

  // Auto-populate `id` from lastOutputId when missing
  if (!hydrated.id && lastOutputId) {
    hydrated.id = lastOutputId
  }

  // For tool_outputs.extract: default `path` to root if missing
  if (toolName === 'tool_outputs.extract') {
    if (!hydrated.path) {
      hydrated.path = '$'
    }
  }

  return hydrated
}
