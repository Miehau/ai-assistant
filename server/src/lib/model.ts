/**
 * Split a "provider:model" identifier into its parts.
 * If no colon is present, both provider and model return the full string.
 */
export function splitModelId(id: string): { provider: string; model: string } {
  const idx = id.indexOf(':')
  return idx === -1
    ? { provider: id, model: id }
    : { provider: id.slice(0, idx), model: id.slice(idx + 1) }
}
