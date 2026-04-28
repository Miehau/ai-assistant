import fs from 'fs/promises'
import path from 'path'
import type { ToolHandler, ToolResult, ToolContext } from './types.js'
import { resolveManagedFilePath } from './path-policy.js'

export interface NoteToolOptions {
  notesDir: string
  sessionFilesRoot: string
}

export function registerNoteTools(
  registry: { register: (h: ToolHandler) => void },
  options: NoteToolOptions,
): void {
  const resolvedNotesDir = path.resolve(options.notesDir)
  const resolvedSessionFilesRoot = path.resolve(options.sessionFilesRoot)

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
      const qualityErrors = validateResearchNoteMarkdown(markdown)
      if (qualityErrors.length > 0) {
        return {
          ok: false,
          error: `Research note failed quality checks: ${qualityErrors.join('; ')}`,
        }
      }

      const filename = requestedFilename
        ? normalizeRequestedFilename(requestedFilename)
        : `${sanitizeFilename(title)}.md`
      return writeNoteFile(resolvedNotesDir, resolvedNotesDir, filename, markdown)
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Save research note "${args.title ?? 'Untitled'}"` }
    },
  })

  registry.register({
    metadata: {
      name: 'notes.promote',
      description: 'Promote existing managed markdown content into the durable notes directory without re-sending the full content. Reads from a relative session workspace path, artifact://..., or @note/....',
      parameters: {
        type: 'object',
        properties: {
          from: { type: 'string', description: 'Managed source path to promote: relative session workspace path, artifact://..., or @note/...' },
          title: { type: 'string', description: 'Human-readable title for the note' },
          filename: {
            type: 'string',
            description: 'Optional relative markdown filename. Absolute paths are rejected.',
          },
          profile: {
            type: 'string',
            description: 'Optional validation profile. Use "research" to require at least one raw http(s) source URL.',
          },
        },
        required: ['from', 'title'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const title = String(args.title ?? '').trim()
      const sourceRef = String(args.from ?? '').trim()
      const requestedFilename = typeof args.filename === 'string' && args.filename.trim()
        ? args.filename.trim()
        : undefined
      const requestedProfile = typeof args.profile === 'string' ? args.profile.trim().toLowerCase() : undefined
      const profile = requestedProfile === 'research' ? 'research' : 'generic'

      if (args.path !== undefined) {
        return { ok: false, error: 'path is not supported; use from with a managed path' }
      }
      if (!title) {
        return { ok: false, error: 'title is required' }
      }
      if (!sourceRef) {
        return { ok: false, error: 'from is required' }
      }

      const resolvedSource = resolveManagedFilePath(sourceRef, {
        sessionFilesRoot: resolvedSessionFilesRoot,
        notesDir: resolvedNotesDir,
        sessionId: ctx.session_id,
        access: 'read',
      })

      let markdown: string
      try {
        markdown = await fs.readFile(resolvedSource.fsPath, 'utf-8')
      } catch (err) {
        return { ok: false, error: `Failed to read source content: ${(err as Error).message}` }
      }

      if (!markdown.trim()) {
        return { ok: false, error: 'source content is empty' }
      }

      const qualityErrors = validateNoteMarkdown(markdown, {
        requireSourceUrl: profile === 'research',
      })
      if (qualityErrors.length > 0) {
        return {
          ok: false,
          error: `Promoted note failed quality checks: ${qualityErrors.join('; ')}`,
        }
      }

      const filename = requestedFilename
        ? normalizeRequestedFilename(requestedFilename)
        : `${sanitizeFilename(title)}.md`
      const written = await writeNoteFile(resolvedNotesDir, resolvedSessionFilesRoot, filename, markdown)
      if (!written.ok) return written

      return {
        ok: true,
        output: {
          ...(written.output as Record<string, unknown>),
          source_path: resolvedSource.ref,
          profile,
        },
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Promote ${args.from} to note "${args.title ?? 'Untitled'}"` }
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

function validateResearchNoteMarkdown(markdown: string): string[] {
  return validateNoteMarkdown(markdown, { requireSourceUrl: true })
}

function validateNoteMarkdown(markdown: string, opts: { requireSourceUrl: boolean }): string[] {
  const errors: string[] = []

  if (/\bturn\d+(?:search|fetch|open|view)\d+\b/i.test(markdown)) {
    errors.push('replace provider placeholder citations such as turn0search0 with raw source URLs')
  }
  if (/[\uE000-\uF8FF]/.test(markdown)) {
    errors.push('remove private citation markers and replace them with raw source URLs')
  }
  if (/artifact:\/\//i.test(markdown)) {
    errors.push('resolve artifact references before saving the final note')
  }
  if (/Output exceeded inline limit and was saved as/i.test(markdown)) {
    errors.push('inspect materialized tool output and summarize the evidence instead of saving the artifact notice')
  }
  if (opts.requireSourceUrl && !/https?:\/\/[^\s)\]>"']+/i.test(markdown)) {
    errors.push('include at least one raw http(s) source URL')
  }

  return errors
}

async function writeNoteFile(
  notesDir: string,
  sessionFilesRoot: string,
  filename: string,
  markdown: string,
): Promise<ToolResult> {
  const outputRef = `@note/${filename}`
  const outputPath = resolveManagedFilePath(outputRef, {
    sessionFilesRoot,
    notesDir,
    sessionId: 'notes',
    access: 'read',
  }).fsPath
  const content = ensureTrailingNewline(markdown)

  try {
    await fs.mkdir(path.dirname(outputPath), { recursive: true })
    await fs.writeFile(outputPath, content, 'utf-8')
    return {
      ok: true,
      output: {
        path: outputRef,
        bytes_written: Buffer.byteLength(content),
      },
    }
  } catch (err) {
    return { ok: false, error: `Failed to save note: ${(err as Error).message}` }
  }
}

function ensureTrailingNewline(content: string): string {
  return content.endsWith('\n') ? content : `${content}\n`
}
