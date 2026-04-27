import type { ToolResult } from '../tools/types.js'
import fs from 'fs/promises'
import path from 'path'
import { getSessionFilesDir } from '../tools/path-policy.js'

const MAX_OUTPUT_BYTES = 32 * 1024 // 32 KB

export interface OutputMaterializationOptions {
  sessionFilesRoot: string
  inlineLimitBytes?: number
  sessionId: string
  agentId: string
  callId: string
  toolName: string
  /**
   * Persist the full text even when it is small enough to inline. Used for
   * delegate outputs so child findings remain inspectable without forcing the
   * parent to ingest large content.
   */
  persistEvenWhenInline?: boolean
}

export async function materializeToolOutput(
  result: ToolResult | ({ call_id: string } & ToolResult),
  options: OutputMaterializationOptions,
): Promise<string> {
  if (!result.ok) {
    return result.error ?? 'Unknown error'
  }
  if (result.output === undefined || result.output === null) {
    return '(no output)'
  }

  const serialized = serializeOutput(result.output)
  return materializeTextOutput(serialized.body, {
    ...options,
    extension: serialized.extension,
  })
}

export async function materializeTextOutput(
  text: string,
  options: OutputMaterializationOptions & { extension?: string },
): Promise<string> {
  const byteLength = Buffer.byteLength(text, 'utf-8')
  const shouldPersist = options.persistEvenWhenInline || byteLength > (options.inlineLimitBytes ?? MAX_OUTPUT_BYTES)

  if (shouldPersist) {
    const artifactRef = await writeArtifact(text, options)
    if (byteLength > (options.inlineLimitBytes ?? MAX_OUTPUT_BYTES)) {
      return artifactReference(artifactRef, text, byteLength)
    }
  }

  if (byteLength <= (options.inlineLimitBytes ?? MAX_OUTPUT_BYTES)) {
    return text
  }

  throw new Error('Large output was not persisted')
}

function serializeOutput(output: unknown): { body: string; extension: string } {
  if (typeof output === 'string') {
    return { body: output, extension: 'txt' }
  }
  return { body: JSON.stringify(output, null, 2), extension: 'json' }
}

async function writeArtifact(
  text: string,
  options: OutputMaterializationOptions & { extension?: string },
): Promise<string> {
  const agentPart = safePathPart(options.agentId)
  const root = path.join(getSessionFilesDir(options.sessionFilesRoot, options.sessionId), 'artifacts')
  const dir = path.join(root, agentPart)
  await fs.mkdir(dir, { recursive: true })

  const filename = [
    safePathPart(options.callId),
    safePathPart(options.toolName),
  ].filter(Boolean).join('-') || `output-${Date.now()}`
  const artifactPath = path.join(dir, `${filename}.${options.extension ?? 'txt'}`)

  await fs.writeFile(artifactPath, text.endsWith('\n') ? text : `${text}\n`, 'utf-8')
  return `artifact://${agentPart}/${path.basename(artifactPath)}`
}

function artifactReference(artifactRef: string, text: string, byteLength: number): string {
  const lineCount = text.length === 0 ? 0 : text.split(/\r\n|\r|\n/).length
  return [
    'Output exceeded inline limit and was saved as:',
    artifactRef,
    '',
    `bytes: ${byteLength}`,
    `lines: ${lineCount}`,
  ].join('\n')
}

function safePathPart(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 96)
}
