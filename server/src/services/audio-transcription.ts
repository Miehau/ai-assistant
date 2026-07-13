import type { RuntimeContext } from '../lib/runtime.js'
import { splitModelId } from '../lib/model.js'

export const MAX_AUDIO_BYTES = 20 * 1024 * 1024

const MIME_FORMATS: Record<string, string> = {
  'audio/aac': 'aac',
  'audio/flac': 'flac',
  'audio/m4a': 'm4a',
  'audio/mp3': 'mp3',
  'audio/mp4': 'm4a',
  'audio/mpeg': 'mp3',
  'audio/ogg': 'ogg',
  'audio/opus': 'ogg',
  'audio/wav': 'wav',
  'audio/wave': 'wav',
  'audio/webm': 'webm',
  'audio/x-m4a': 'm4a',
  'audio/x-wav': 'wav',
}

const EXTENSION_FORMATS: Record<string, string> = {
  aac: 'aac',
  flac: 'flac',
  m4a: 'm4a',
  mp3: 'mp3',
  mp4: 'm4a',
  oga: 'ogg',
  ogg: 'ogg',
  opus: 'ogg',
  wav: 'wav',
  webm: 'webm',
}

export class AudioInputError extends Error {}

export interface AudioTranscriptionInput {
  bytes: ArrayBuffer | Uint8Array
  mimeType?: string
  fileName?: string
  signal?: AbortSignal
}

export function inferAudioFormat(mimeType?: string, fileName?: string): string {
  const normalizedMime = mimeType?.toLowerCase().split(';', 1)[0]?.trim()
  if (normalizedMime && MIME_FORMATS[normalizedMime]) return MIME_FORMATS[normalizedMime]

  const extension = fileName?.toLowerCase().split('.').pop()
  if (extension && EXTENSION_FORMATS[extension]) return EXTENSION_FORMATS[extension]

  throw new AudioInputError(`Unsupported audio format${fileName ? `: ${fileName}` : ''}`)
}

export function isSupportedAudioFile(mimeType?: string, fileName?: string): boolean {
  try {
    inferAudioFormat(mimeType, fileName)
    return true
  } catch {
    return false
  }
}

export async function transcribeAudio(
  runtime: RuntimeContext,
  input: AudioTranscriptionInput,
): Promise<{ text: string; usage?: unknown }> {
  const size = input.bytes.byteLength
  if (size === 0) throw new AudioInputError('Audio file is empty')
  if (size > MAX_AUDIO_BYTES) throw new AudioInputError(`Audio file is too large (${size} bytes)`)

  const format = inferAudioFormat(input.mimeType, input.fileName)
  const transcriptionModel = runtime.config.audioTranscriptionModel?.trim() ||
    runtime.config.telegramTranscriptionModel?.trim()
  if (!transcriptionModel) {
    throw new Error('AUDIO_TRANSCRIPTION_MODEL is not configured')
  }

  const { provider, model } = splitModelId(transcriptionModel)
  const transcriptionProvider = runtime.providers.resolve(transcriptionModel)
  if (!transcriptionProvider.transcribeAudio) {
    throw new Error(`Provider "${provider}" does not support audio transcription`)
  }

  const response = await transcriptionProvider.transcribeAudio({
    model,
    input_audio: {
      data: Buffer.from(input.bytes instanceof ArrayBuffer ? new Uint8Array(input.bytes) : input.bytes).toString('base64'),
      format,
    },
    signal: input.signal ?? runtime.shutdownController.signal,
  })
  const text = response.text.trim()
  if (!text) throw new Error('Audio transcription response was empty')
  return { text, usage: response.usage }
}
