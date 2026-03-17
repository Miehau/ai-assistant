import fs from 'fs/promises'
import path from 'path'
import type { ToolHandler, ToolResult, ToolContext } from './types.js'

const DEFAULT_MAX_LINES = 200

const IMAGE_EXTENSIONS: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
}

export function registerFileTools(registry: { register: (h: ToolHandler) => void }): void {
  registry.register({
    metadata: {
      name: 'files.read',
      description: 'Read file content. Returns lines from the text file, optionally within a range. Loads images.',
      parameters: {
        type: 'object',
        properties: {
          path: { type: 'string', description: 'Absolute path to the file' },
          start_line: { type: 'integer', description: 'Start line (1-based, inclusive)' },
          end_line: { type: 'integer', description: 'End line (1-based, inclusive)' },
        },
        required: ['path'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const filePath = args.path as string
      const startLine = (args.start_line as number | undefined) ?? 1
      const endLine = args.end_line as number | undefined

      try {
        const ext = path.extname(filePath).toLowerCase()
        const mediaType = IMAGE_EXTENSIONS[ext]

        if (mediaType) {
          const buffer = await fs.readFile(filePath)
          return {
            ok: true,
            output: { path: filePath, media_type: mediaType },
            content_blocks: [
              { type: 'text', text: `Image file: ${filePath} (${mediaType})` },
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
            path: filePath,
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
          path: { type: 'string', description: 'Absolute path to the file' },
          content: { type: 'string', description: 'Content to write' },
        },
        required: ['path', 'content'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const filePath = args.path as string
      const content = args.content as string

      try {
        await fs.mkdir(path.dirname(filePath), { recursive: true })
        await fs.writeFile(filePath, content, 'utf-8')
        return { ok: true, output: { path: filePath, bytes_written: Buffer.byteLength(content) } }
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
          path: { type: 'string', description: 'Absolute path to the file' },
          old_text: { type: 'string', description: 'Text to find' },
          new_text: { type: 'string', description: 'Replacement text' },
        },
        required: ['path', 'old_text', 'new_text'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const filePath = args.path as string
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

        return { ok: true, output: { path: filePath, replaced: true } }
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
          path: { type: 'string', description: 'Absolute path for the new file' },
          content: { type: 'string', description: 'File content' },
        },
        required: ['path', 'content'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const filePath = args.path as string
      const content = args.content as string

      try {
        try {
          await fs.access(filePath)
          return { ok: false, error: `File already exists: ${filePath}` }
        } catch {
          // File does not exist — proceed
        }

        await fs.mkdir(path.dirname(filePath), { recursive: true })
        await fs.writeFile(filePath, content, 'utf-8')
        return { ok: true, output: { path: filePath, bytes_written: Buffer.byteLength(content) } }
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
          path: { type: 'string', description: 'Absolute path to the file' },
          content: { type: 'string', description: 'Content to append' },
        },
        required: ['path', 'content'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const filePath = args.path as string
      const content = args.content as string

      try {
        await fs.appendFile(filePath, content, 'utf-8')
        return { ok: true, output: { path: filePath, bytes_appended: Buffer.byteLength(content) } }
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
          path: { type: 'string', description: 'Directory path to list' },
          recursive: { type: 'boolean', description: 'List recursively (default: false)' },
        },
        required: ['path'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>): Promise<ToolResult> {
      const dirPath = args.path as string
      const recursive = (args.recursive as boolean) ?? false

      try {
        const entries = await listDir(dirPath, recursive, dirPath)
        return { ok: true, output: { path: dirPath, count: entries.length, entries } }
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
  type: 'file' | 'directory'
  size: number
}

async function listDir(dirPath: string, recursive: boolean, basePath: string): Promise<DirEntry[]> {
  const dirents = await fs.readdir(dirPath, { withFileTypes: true })
  const results: DirEntry[] = []

  for (const dirent of dirents) {
    const fullPath = path.join(dirPath, dirent.name)
    const relativeName = path.relative(basePath, fullPath)

    if (dirent.isDirectory()) {
      results.push({ name: relativeName, type: 'directory', size: 0 })
      if (recursive) {
        const sub = await listDir(fullPath, true, basePath)
        results.push(...sub)
      }
    } else if (dirent.isFile()) {
      const stat = await fs.stat(fullPath)
      results.push({ name: relativeName, type: 'file', size: stat.size })
    }
  }

  return results
}
