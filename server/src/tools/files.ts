import fs from 'fs/promises'
import path from 'path'
import type { ToolHandler, ToolResult, ToolContext } from './types.js'
import {
  joinManagedFileRef,
  managedFileRefForPath,
  resolveManagedFilePath,
  type ResolvedManagedFile,
} from './path-policy.js'

const DEFAULT_MAX_LINES = 200

const IMAGE_EXTENSIONS: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
}

export interface FileToolOptions {
  sessionFilesRoot: string
  notesDir: string
}

export function registerFileTools(
  registry: { register: (h: ToolHandler) => void },
  options: FileToolOptions,
): void {
  registry.register({
    metadata: {
      name: 'files.read',
      description: 'Read file content. Returns lines from the text file, optionally within a range. Loads images.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Managed path to read: relative session workspace path, artifact://..., or note://...' },
          start_line: { type: 'integer', description: 'Start line (1-based, inclusive)' },
          end_line: { type: 'integer', description: 'End line (1-based, inclusive)' },
        },
        required: ['path'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const resolved = resolveManagedFilePath(args.path as string, {
        sessionFilesRoot: options.sessionFilesRoot,
        notesDir: options.notesDir,
        sessionId: ctx.session_id,
        access: 'read',
      })
      const filePath = resolved.fsPath
      const startLine = (args.start_line as number | undefined) ?? 1
      const endLine = args.end_line as number | undefined

      try {
        const ext = path.extname(filePath).toLowerCase()
        const mediaType = IMAGE_EXTENSIONS[ext]

        if (mediaType) {
          const buffer = await fs.readFile(filePath)
          return {
            ok: true,
            output: { path: resolved.ref, media_type: mediaType },
            content_blocks: [
              { type: 'text', text: `Image file: ${resolved.ref} (${mediaType})` },
              { type: 'image', media_type: mediaType, data: buffer.toString('base64') },
            ],
          }
        }

        const content = await fs.readFile(filePath, 'utf-8')
        const lines = content.split('\n')
        const start = Math.max(1, startLine) - 1
        const end = endLine ? Math.min(endLine, lines.length) : Math.min(start + DEFAULT_MAX_LINES, lines.length)
        const sliced = lines.slice(start, end)
        const numbered = sliced.map((line, i) => `${start + i + 1}\t${line}`)

        return {
          ok: true,
          output: {
            path: resolved.ref,
            start_line: start + 1,
            end_line: end,
            total_lines: lines.length,
            content: numbered.join('\n'),
          },
        }
      } catch (err) {
        return { ok: false, error: `Failed to read file: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string; details?: Record<string, unknown> } {
      const range = args.start_line ? ` lines ${args.start_line}-${args.end_line ?? '...'}` : ''
      return { summary: `Read ${args.path}${range}` }
    },
  })

  registry.register({
    metadata: {
      name: 'files.write',
      description: 'Write content to a file. Creates parent directories if needed.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Relative session workspace path to write. artifact:// and note:// are read-only.' },
          content: { type: 'string', description: 'Content to write' },
        },
        required: ['path', 'content'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const resolved = resolveManagedFilePath(args.path as string, {
        sessionFilesRoot: options.sessionFilesRoot,
        notesDir: options.notesDir,
        sessionId: ctx.session_id,
        access: 'write',
      })
      const filePath = resolved.fsPath
      const content = args.content as string

      try {
        await fs.mkdir(path.dirname(filePath), { recursive: true })
        await fs.writeFile(filePath, content, 'utf-8')
        return { ok: true, output: { path: resolved.ref, bytes_written: Buffer.byteLength(content) } }
      } catch (err) {
        return { ok: false, error: `Failed to write file: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Write ${Buffer.byteLength(String(args.content))} bytes to ${args.path}` }
    },
  })

  registry.register({
    metadata: {
      name: 'files.edit',
      description: 'Search and replace text in a file. Replaces the first occurrence of old_text with new_text.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Relative session workspace path to edit. artifact:// and note:// are read-only.' },
          old_text: { type: 'string', description: 'Text to find' },
          new_text: { type: 'string', description: 'Replacement text' },
        },
        required: ['path', 'old_text', 'new_text'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const resolved = resolveManagedFilePath(args.path as string, {
        sessionFilesRoot: options.sessionFilesRoot,
        notesDir: options.notesDir,
        sessionId: ctx.session_id,
        access: 'write',
      })
      const filePath = resolved.fsPath
      const oldText = args.old_text as string
      const newText = args.new_text as string

      try {
        const content = await fs.readFile(filePath, 'utf-8')
        if (!content.includes(oldText)) {
          return { ok: false, error: 'old_text not found in file' }
        }

        const idx = content.indexOf(oldText)
        const updated = content.slice(0, idx) + newText + content.slice(idx + oldText.length)
        await fs.writeFile(filePath, updated, 'utf-8')

        return { ok: true, output: { path: resolved.ref, replaced: true } }
      } catch (err) {
        return { ok: false, error: `Failed to edit file: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Edit ${args.path}: replace text` }
    },
  })

  registry.register({
    metadata: {
      name: 'files.create',
      description: 'Create a new file. Errors if the file already exists.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Relative session workspace path for the new file. artifact:// and note:// are read-only.' },
          content: { type: 'string', description: 'File content' },
        },
        required: ['path', 'content'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const resolved = resolveManagedFilePath(args.path as string, {
        sessionFilesRoot: options.sessionFilesRoot,
        notesDir: options.notesDir,
        sessionId: ctx.session_id,
        access: 'write',
      })
      const filePath = resolved.fsPath
      const content = args.content as string

      try {
        try {
          await fs.access(filePath)
          return { ok: false, error: `File already exists: ${resolved.ref}` }
        } catch {
          // File does not exist — proceed
        }

        await fs.mkdir(path.dirname(filePath), { recursive: true })
        await fs.writeFile(filePath, content, 'utf-8')
        return { ok: true, output: { path: resolved.ref, bytes_written: Buffer.byteLength(content) } }
      } catch (err) {
        return { ok: false, error: `Failed to create file: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Create ${args.path}` }
    },
  })

  registry.register({
    metadata: {
      name: 'files.append',
      description: 'Append content to an existing file.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Relative session workspace path to append to. artifact:// and note:// are read-only.' },
          content: { type: 'string', description: 'Content to append' },
        },
        required: ['path', 'content'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const resolved = resolveManagedFilePath(args.path as string, {
        sessionFilesRoot: options.sessionFilesRoot,
        notesDir: options.notesDir,
        sessionId: ctx.session_id,
        access: 'write',
      })
      const filePath = resolved.fsPath
      const content = args.content as string

      try {
        await fs.appendFile(filePath, content, 'utf-8')
        return { ok: true, output: { path: resolved.ref, bytes_appended: Buffer.byteLength(content) } }
      } catch (err) {
        return { ok: false, error: `Failed to append to file: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Append to ${args.path}` }
    },
  })

  registry.register({
    metadata: {
      name: 'files.list',
      description: 'List files and directories. Returns names and sizes.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Managed directory path to list: relative session workspace path, artifact://..., or note://...' },
          recursive: { type: 'boolean', description: 'List recursively (default: false)' },
        },
        required: ['path'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const resolved = resolveManagedFilePath(args.path as string, {
        sessionFilesRoot: options.sessionFilesRoot,
        notesDir: options.notesDir,
        sessionId: ctx.session_id,
        access: 'read',
      })
      const dirPath = resolved.fsPath
      const recursive = (args.recursive as boolean) ?? false

      try {
        const entries = await listDir(dirPath, recursive, dirPath, resolved)
        return { ok: true, output: { path: resolved.ref, count: entries.length, entries } }
      } catch (err) {
        return { ok: false, error: `Failed to list directory: ${(err as Error).message}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `List ${args.path}${args.recursive ? ' (recursive)' : ''}` }
    },
  })
}

interface DirEntry {
  name: string
  path: string
  type: 'file' | 'directory'
  size: number
}

async function listDir(
  dirPath: string,
  recursive: boolean,
  basePath: string,
  resolvedBase: ResolvedManagedFile,
): Promise<DirEntry[]> {
  const dirents = await fs.readdir(dirPath, { withFileTypes: true })
  const results: DirEntry[] = []

  for (const dirent of dirents) {
    const fullPath = path.join(dirPath, dirent.name)
    const relativeName = path.relative(basePath, fullPath)
    const logicalPath = joinManagedFileRef(resolvedBase.ref, relativeName.split(path.sep).join('/'))

    if (dirent.isDirectory()) {
      results.push({ name: relativeName, path: logicalPath, type: 'directory', size: 0 })
      if (recursive) {
        const sub = await listDir(fullPath, true, basePath, resolvedBase)
        results.push(...sub)
      }
    } else if (dirent.isFile()) {
      const stat = await fs.stat(fullPath)
      results.push({
        name: relativeName,
        path: managedFileRefForPath(fullPath, resolvedBase),
        type: 'file',
        size: stat.size,
      })
    }
  }

  return results
}
