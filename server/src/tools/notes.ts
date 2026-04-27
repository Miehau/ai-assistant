import fs from 'fs/promises'
import path from 'path'
import type { ToolHandler, ToolResult } from './types.js'
import { resolveManagedFilePath } from './path-policy.js'

export function registerNoteTools(
  registry: { register: (h: ToolHandler) => void },
  notesDir: string,
): void {
  const resolvedNotesDir = path.resolve(notesDir)

  registry.register({
    metadata: {
      name: 'notes.save_research_note',
      description: 'Save a completed research report as a markdown note under the managed research-notes directory.',
      parameters: {
        type: 'object',
        properties: {
          title: { type: 'string', description: 'Human-readable title for the research note' },
          markdown: { type: 'string', description: 'Complete markdown note body to save' },
          filename: {
            type: 'string',
            description: 'Optional relative markdown filename. Absolute paths are rejected.',
          },
        },
        required: ['title', 'markdown'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const title = String(args.title ?? '').trim()
      const markdown = String(args.markdown ?? '')
      if (args.path !== undefined) {
        return { ok: false, error: 'path is not supported; use filename with a relative name' }
      }
      const requestedFilename = typeof args.filename === 'string' && args.filename.trim()
        ? args.filename.trim()
        : undefined

      if (!title) {
        return { ok: false, error: 'title is required' }
      }
      if (!markdown.trim()) {
        return { ok: false, error: 'markdown is required' }
      }

      const filename = requestedFilename
        ? normalizeRequestedFilename(requestedFilename)
        : `${sanitizeFilename(title)}.md`
      const outputRef = `note://${filename}`
      const outputPath = resolveManagedFilePath(outputRef, {
        sessionFilesRoot: resolvedNotesDir,
        notesDir: resolvedNotesDir,
        sessionId: 'notes',
        access: 'read',
      }).fsPath

      try {
        await fs.mkdir(path.dirname(outputPath), { recursive: true })
        await fs.writeFile(outputPath, ensureTrailingNewline(markdown), 'utf-8')
        return {
          ok: true,
          output: {
            path: outputRef,
            bytes_written: Buffer.byteLength(ensureTrailingNewline(markdown)),
          },
        }
      } catch (err) {
        return { ok: false, error: `Failed to save research note: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Save research note "${args.title ?? 'Untitled'}"` }
    },
  })
}

function sanitizeFilename(title: string): string {
  const sanitized = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80)
  return sanitized || `research-note-${Date.now()}`
}

function ensureMarkdownExtension(filename: string): string {
  return filename.endsWith('.md') ? filename : `${filename}.md`
}

function normalizeRequestedFilename(filename: string): string {
  if (path.isAbsolute(filename) || filename.includes('/') || filename.includes('\\')) {
    throw new Error('filename must be a relative file name, not a path')
  }
  const withoutExtension = filename.endsWith('.md') ? filename.slice(0, -3) : filename
  return ensureMarkdownExtension(sanitizeFilename(withoutExtension))
}

function ensureTrailingNewline(content: string): string {
  return content.endsWith('\n') ? content : `${content}\n`
}
