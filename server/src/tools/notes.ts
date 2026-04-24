import fs from 'fs/promises'
import path from 'path'
import type { ToolHandler, ToolResult } from './types.js'

export function registerNoteTools(
  registry: { register: (h: ToolHandler) => void },
  notesDir: string,
  allowedRoots: string[],
): void {
  const resolvedNotesDir = path.resolve(notesDir)
  const roots = Array.from(new Set([resolvedNotesDir, ...allowedRoots.map((root) => path.resolve(root))]))

  registry.register({
    metadata: {
      name: 'notes.save_research_note',
      description: 'Save a completed research report as a markdown note under an allowed notes/workspace directory.',
      parameters: {
        type: 'object',
        properties: {
          title: { type: 'string', description: 'Human-readable title for the research note' },
          markdown: { type: 'string', description: 'Complete markdown note body to save' },
          path: {
            type: 'string',
            description: 'Optional absolute output path. Must resolve under an allowed notes/workspace directory.',
          },
        },
        required: ['title', 'markdown'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const title = String(args.title ?? '').trim()
      const markdown = String(args.markdown ?? '')
      const requestedPath = typeof args.path === 'string' && args.path.trim()
        ? args.path.trim()
        : undefined

      if (!title) {
        return { ok: false, error: 'title is required' }
      }
      if (!markdown.trim()) {
        return { ok: false, error: 'markdown is required' }
      }

      const filename = `${sanitizeFilename(title)}.md`
      const outputPath = requestedPath
        ? path.resolve(requestedPath)
        : path.join(resolvedNotesDir, filename)

      if (!isUnderAllowedRoot(outputPath, roots)) {
        return {
          ok: false,
          error: `Refusing to write outside allowed note roots: ${roots.join(', ')}`,
        }
      }

      try {
        await fs.mkdir(path.dirname(outputPath), { recursive: true })
        await fs.writeFile(outputPath, ensureTrailingNewline(markdown), 'utf-8')
        return {
          ok: true,
          output: {
            path: outputPath,
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

function isUnderAllowedRoot(candidate: string, roots: string[]): boolean {
  const resolvedCandidate = path.resolve(candidate)
  return roots.some((root) => {
    const resolvedRoot = path.resolve(root)
    return resolvedCandidate === resolvedRoot || resolvedCandidate.startsWith(`${resolvedRoot}${path.sep}`)
  })
}

function ensureTrailingNewline(content: string): string {
  return content.endsWith('\n') ? content : `${content}\n`
}
