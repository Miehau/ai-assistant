const PRIVATE_PROVIDER_CITATION_RE =
  /\uE200(?:[a-z_]+)?cite(?:\uE202[^\uE200\uE201\uE202]+)+\uE201/gi

const PROVIDER_PLACEHOLDER_ID_RE = /\bturn\d+(?:search|fetch|open|view)\d+\b/gi
const PRIVATE_USE_RE = /[\uE000-\uF8FF]/
const STREAM_TAIL_LENGTH = 64

/**
 * Remove provider-native citation artifacts from model text.
 *
 * Hosted web-search providers can emit private-use Unicode markers whose inner
 * references only make sense inside a provider trace. They are not resolvable
 * source URLs, and they degrade later model turns when stored in conversation
 * history. Provider adapters call this before returning text to the
 * orchestrator, deleting the marker and any bare placeholder IDs before
 * tidying the whitespace left behind.
 */
export function sanitizeProviderCitations(text: string): string {
  return cleanCitationWhitespace(
    text
      .replace(PRIVATE_PROVIDER_CITATION_RE, '')
      .replace(PROVIDER_PLACEHOLDER_ID_RE, ''),
  )
}

/**
 * Detect artifacts that must not be accepted in durable research outputs.
 *
 * Sanitizing final chat text is fine, but saved research notes need real URLs,
 * not silently removed citation markers. Validators use this as a quality gate
 * so the agent repairs the note with human/machine-readable sources.
 */
export function containsProviderCitationArtifacts(text: string): boolean {
  PRIVATE_PROVIDER_CITATION_RE.lastIndex = 0
  PROVIDER_PLACEHOLDER_ID_RE.lastIndex = 0
  return PRIVATE_PROVIDER_CITATION_RE.test(text) ||
    PROVIDER_PLACEHOLDER_ID_RE.test(text) ||
    PRIVATE_USE_RE.test(text)
}

/**
 * Streaming variant for provider adapters.
 *
 * Citation markers and placeholder IDs can be split across provider stream
 * chunks. This keeps a small tail buffered, and it holds an unfinished
 * private-use citation marker until its closing marker arrives. `flush()`
 * emits the final cleaned tail at the end of the stream.
 */
export class ProviderCitationStreamSanitizer {
  private buffer = ''

  push(chunk: string): string {
    this.buffer += chunk

    const openMarker = this.buffer.lastIndexOf('\uE200')
    if (openMarker !== -1 && this.buffer.indexOf('\uE201', openMarker) === -1) {
      const safe = this.buffer.slice(0, openMarker)
      this.buffer = this.buffer.slice(openMarker)
      return sanitizeProviderCitations(safe)
    }

    if (this.buffer.length <= STREAM_TAIL_LENGTH) return ''

    const safeEnd = this.buffer.length - STREAM_TAIL_LENGTH
    const safe = this.buffer.slice(0, safeEnd)
    this.buffer = this.buffer.slice(safeEnd)
    return sanitizeProviderCitations(safe)
  }

  flush(): string {
    const text = sanitizeProviderCitations(this.buffer)
    this.buffer = ''
    return text
  }
}

function cleanCitationWhitespace(text: string): string {
  return text
    .replace(/[ \t]+([,.;:!?])/g, '$1')
    .replace(/\(\s*\)/g, '')
    .replace(/\[\s*\]/g, '')
    .replace(/[ \t]{2,}/g, ' ')
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trimEnd()
}
