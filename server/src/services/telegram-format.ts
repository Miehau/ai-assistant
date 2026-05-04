const TELEGRAM_MAX_LENGTH = 3900

const SIMPLE_TELEGRAM_HTML_TAGS = new Set([
  'b',
  'strong',
  'i',
  'em',
  'u',
  'ins',
  's',
  'strike',
  'del',
  'code',
  'pre',
])

export function truncateTelegram(text: string, maxLength = TELEGRAM_MAX_LENGTH): string {
  return text.length > maxLength
    ? `${text.slice(0, maxLength - 40).trimEnd()}\n\n[truncated]`
    : text
}

export function formatTelegramHtml(text: string): string {
  const truncated = truncateTelegram(text)
  let html = ''
  let index = 0
  const tagPattern = /<\/?[A-Za-z][A-Za-z0-9]*(?:\s+[^<>]*)?>/g

  for (const match of truncated.matchAll(tagPattern)) {
    const rawTag = match[0]
    const start = match.index ?? 0
    html += escapeTelegramHtml(truncated.slice(index, start))
    html += renderTelegramTag(rawTag) ?? escapeTelegramHtml(rawTag)
    index = start + rawTag.length
  }

  html += escapeTelegramHtml(truncated.slice(index))
  return html
}

export function telegramHtmlToPlainText(html: string): string {
  return decodeTelegramEntities(html.replace(/<\/?[A-Za-z][A-Za-z0-9]*(?:\s+[^<>]*)?>/g, ''))
}

function renderTelegramTag(rawTag: string): string | null {
  const closing = /^<\s*\//.test(rawTag)
  const nameMatch = rawTag.match(/^<\s*\/?\s*([A-Za-z][A-Za-z0-9]*)/)
  const name = nameMatch?.[1]?.toLowerCase()
  if (!name) return null

  if (name === 'a') {
    if (closing) {
      return /^<\s*\/\s*a\s*>$/i.test(rawTag) ? '</a>' : null
    }
    const hrefMatch = rawTag.match(/\s+href\s*=\s*(?:"([^"]*)"|'([^']*)')/i)
    const href = hrefMatch?.[1] ?? hrefMatch?.[2]
    if (!href || !/^https?:\/\//i.test(href)) return null
    return `<a href="${escapeTelegramAttribute(href)}">`
  }

  if (!SIMPLE_TELEGRAM_HTML_TAGS.has(name)) return null
  const simplePattern = closing
    ? new RegExp(`^<\\s*\\/\\s*${name}\\s*>$`, 'i')
    : new RegExp(`^<\\s*${name}\\s*>$`, 'i')
  if (!simplePattern.test(rawTag)) return null

  return closing ? `</${name}>` : `<${name}>`
}

function escapeTelegramHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

function escapeTelegramAttribute(text: string): string {
  return escapeTelegramHtml(text).replace(/"/g, '&quot;')
}

function decodeTelegramEntities(text: string): string {
  return text
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, '&')
}
