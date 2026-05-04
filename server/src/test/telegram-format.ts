import assert from 'node:assert/strict'
import { formatTelegramHtml, splitTelegramText } from '../services/telegram-format.js'

const longText = [
  'First paragraph with useful context.',
  '',
  'Second paragraph should move to another message when the limit is small.',
  'Final sentence.',
].join('\n')

const chunks = splitTelegramText(longText, 48)
assert(chunks.length > 1)
assert(chunks.every((chunk) => chunk.length <= 48))
assert.equal(chunks.join('\n').includes('[truncated]'), false)
assert.equal(chunks.join('\n').includes('Second paragraph'), true)

const html = formatTelegramHtml('<b>safe</b> & <script>escaped</script>')
assert.equal(html, '<b>safe</b> &amp; &lt;script&gt;escaped&lt;/script&gt;')

console.log('Telegram format tests passed')
