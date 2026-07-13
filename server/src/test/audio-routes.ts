import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { Hono } from 'hono'
import type { RuntimeContext } from '../lib/runtime.js'
import { audioRoutes } from '../routes/audio.js'
import { authMiddleware } from '../middleware/auth.js'

const token = 'audio-route-token'
const tokenHash = createHash('sha256').update(token).digest('hex')

const runtime = {
  config: { audioTranscriptionModel: 'openrouter:test-stt' },
  providers: {
    resolve() {
      return {
        async transcribeAudio() {
          return { text: 'uploaded audio transcript' }
        },
      }
    },
  },
  repositories: {
    users: {
      async getByApiKeyHash(hash: string) {
        return hash === tokenHash ? { id: 'audio-user' } : null
      },
    },
  },
  shutdownController: new AbortController(),
} as unknown as RuntimeContext

type TestEnv = { Variables: { runtime: RuntimeContext; userId: string } }
const app = new Hono<TestEnv>()
app.use('*', async (c, next) => {
  c.set('runtime', runtime)
  await next()
})
app.use('/api/*', authMiddleware)
app.route('/api/audio', audioRoutes(runtime))

assert.equal((await app.request('/api/audio/transcriptions', { method: 'POST' })).status, 401)

const authorized = (body?: FormData) => app.request('/api/audio/transcriptions', {
  method: 'POST',
  headers: { Authorization: `Bearer ${token}` },
  body,
})

assert.equal((await authorized()).status, 400)

const unsupported = new FormData()
unsupported.append('file', new File(['hello'], 'notes.txt', { type: 'text/plain' }))
assert.equal((await authorized(unsupported)).status, 400)

const valid = new FormData()
valid.append('file', new File([new Uint8Array([1, 2, 3])], 'voice.ogg', { type: 'audio/ogg' }))
const response = await authorized(valid)
assert.equal(response.status, 200)
assert.deepEqual(await response.json(), { text: 'uploaded audio transcript' })

console.log('Audio transcription route tests passed')
