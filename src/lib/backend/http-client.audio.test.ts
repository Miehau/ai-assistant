import assert from 'node:assert/strict'
import { HttpBackendClient } from './http-client'
import { isAudioFile } from '../types/attachments'

assert.equal(isAudioFile({ name: 'recording.m4a', type: 'application/octet-stream' }), true)
assert.equal(isAudioFile({ name: 'voice.opus', type: '' }), true)
assert.equal(isAudioFile({ name: 'document.pdf', type: 'application/pdf' }), false)

const originalFetch = globalThis.fetch
const controller = new AbortController()

try {
  globalThis.fetch = async (input, init) => {
    assert.equal(String(input), 'http://audio.test/api/audio/transcriptions')
    assert.equal(init?.method, 'POST')
    assert.equal((init?.headers as Record<string, string>).Authorization, 'Bearer test-token')
    assert.equal((init?.headers as Record<string, string>)['Content-Type'], undefined)
    assert.equal(init?.signal, controller.signal)
    assert(init?.body instanceof FormData)
    const file = init.body.get('file')
    assert(file instanceof File)
    assert.equal(file.name, 'voice.ogg')
    return Response.json({ text: 'client transcript' })
  }

  const client = new HttpBackendClient({ serverUrl: 'http://audio.test', token: 'test-token' })
  const result = await client.transcribeAudio(
    new Blob(['audio'], { type: 'audio/ogg' }),
    'voice.ogg',
    controller.signal,
  )
  assert.deepEqual(result, { text: 'client transcript' })
  console.log('Audio HTTP client tests passed')
} finally {
  globalThis.fetch = originalFetch
}
