import { Hono } from 'hono'
import { bodyLimit } from 'hono/body-limit'
import type { RuntimeContext } from '../lib/runtime.js'
import {
  AudioInputError,
  MAX_AUDIO_BYTES,
  transcribeAudio,
} from '../services/audio-transcription.js'

type AudioEnv = { Variables: { userId: string } }

export function audioRoutes(runtime: RuntimeContext): Hono<AudioEnv> {
  const app = new Hono<AudioEnv>()

  app.post(
    '/transcriptions',
    bodyLimit({
      maxSize: MAX_AUDIO_BYTES + 1024 * 1024,
      onError: (c) => c.json({ error: 'Audio upload is too large' }, 413),
    }),
    async (c) => {
      try {
        const body = await c.req.parseBody()
        const file = body.file
        if (!(file instanceof File)) {
          return c.json({ error: 'A multipart audio file is required' }, 400)
        }

        const result = await transcribeAudio(runtime, {
          bytes: await file.arrayBuffer(),
          mimeType: file.type,
          fileName: file.name,
          signal: c.req.raw.signal,
        })
        return c.json(result)
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        return c.json({ error: message }, err instanceof AudioInputError ? 400 : 500)
      }
    },
  )

  return app
}
