import assert from 'node:assert/strict'
import type { RuntimeContext } from '../lib/runtime.js'
import {
  AudioInputError,
  inferAudioFormat,
  isSupportedAudioFile,
  MAX_AUDIO_BYTES,
  transcribeAudio,
} from '../services/audio-transcription.js'

assert.equal(inferAudioFormat('audio/ogg; codecs=opus'), 'ogg')
assert.equal(inferAudioFormat(undefined, 'meeting.M4A'), 'm4a')
assert.equal(inferAudioFormat('application/octet-stream', 'voice.opus'), 'ogg')
assert.equal(isSupportedAudioFile('text/plain', 'notes.txt'), false)
assert.throws(() => inferAudioFormat('audio/unknown', 'recording.bin'), AudioInputError)

let captured: Record<string, unknown> | undefined
const runtime = {
  config: {
    audioTranscriptionModel: 'openrouter:openai/whisper-1',
  },
  providers: {
    resolve(model: string) {
      assert.equal(model, 'openrouter:openai/whisper-1')
      return {
        async transcribeAudio(request: Record<string, unknown>) {
          captured = request
          return { text: '  normalized transcript  ', usage: { seconds: 2 } }
        },
      }
    },
  },
  shutdownController: new AbortController(),
} as unknown as RuntimeContext

const result = await transcribeAudio(runtime, {
  bytes: new Uint8Array([1, 2, 3]),
  mimeType: 'audio/ogg',
  fileName: 'voice.ogg',
})
assert.deepEqual(result, { text: 'normalized transcript', usage: { seconds: 2 } })
assert.deepEqual(captured, {
  model: 'openai/whisper-1',
  input_audio: { data: 'AQID', format: 'ogg' },
  signal: runtime.shutdownController.signal,
})

await assert.rejects(
  () => transcribeAudio(runtime, { bytes: new Uint8Array(), fileName: 'empty.wav' }),
  AudioInputError,
)
await assert.rejects(
  () => transcribeAudio(runtime, {
    bytes: new Uint8Array(MAX_AUDIO_BYTES + 1),
    fileName: 'large.mp3',
  }),
  /too large/,
)

console.log('Audio transcription service tests passed')
