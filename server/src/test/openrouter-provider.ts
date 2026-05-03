import assert from 'node:assert/strict'
import {
  buildAudioTranscriptionRequestBody,
  buildRequestBody,
  preflightAudioTranscriptionRequestBody,
  preflightRequestBody,
} from '../providers/openrouter.js'
import type { LLMRequest } from '../providers/types.js'

const request: LLMRequest = {
  model: 'openai/gpt-5.4-mini',
  messages: [{ role: 'user', content: 'Find current sources and summarize them.' }],
  tools: [{
    name: 'web_search',
    description: 'Provider-native web search',
    parameters: { type: 'object', properties: {} },
  }, {
    name: 'web.fetch',
    description: 'Fetch a URL',
    parameters: {
      type: 'object',
      properties: {
        url: { type: 'string' },
      },
      required: ['url'],
    },
  }],
}

const body = buildRequestBody(request)
preflightRequestBody(body)

assert.equal(body.max_tokens, 12_000)
assert.deepEqual((body.tools as unknown[])[0], { type: 'openrouter:web_search' })
assert.equal((body.tools as Array<Record<string, unknown>>)[1].type, 'function')
assert.deepEqual(
  ((body.tools as Array<{ function?: { name?: string } }>)[1].function?.name),
  'web__fetch',
)

const limitedBody = buildRequestBody({ ...request, max_tokens: 2048 })
preflightRequestBody(limitedBody)
assert.equal(limitedBody.max_tokens, 2048)

assert.throws(
  () => preflightRequestBody({
    model: 'openai/gpt-5.4-mini',
    messages: [],
    tools: [{ type: 'web_search' }],
  }),
  /unsupported tools\[0\] shape/,
)

const transcriptionBody = buildAudioTranscriptionRequestBody({
  model: 'openai/whisper-1',
  input_audio: {
    data: Buffer.from('audio-bytes').toString('base64'),
    format: 'ogg',
  },
})
preflightAudioTranscriptionRequestBody(transcriptionBody)
assert.deepEqual(transcriptionBody, {
  model: 'openai/whisper-1',
  input_audio: {
    data: Buffer.from('audio-bytes').toString('base64'),
    format: 'ogg',
  },
})

assert.throws(
  () => preflightAudioTranscriptionRequestBody({
    model: 'openai/whisper-1',
    input_audio: {
      data: 'data:audio/ogg;base64,abc123',
      format: 'ogg',
    },
  }),
  /must be raw base64/,
)

console.log('OpenRouter provider payload tests passed')
