import { execFile } from 'child_process'
import { promisify } from 'util'
import type { ToolHandler, ToolResult, ToolContext } from './types.js'

const execFileAsync = promisify(execFile)

const MAX_RESULTS_DEFAULT = 200

export function registerSearchTools(registry: { register: (h: ToolHandler) => void }): void {
  registry.register({
    metadata: {
      name: 'search',
      description:
        'Search file contents using grep. Returns matching lines with file path, line number, and snippet.',
      parameters: {
        type: 'object',
        properties: {
          query: { type: 'string', description: 'Search pattern (regex unless literal is true)' },
          path: { type: 'string', description: 'Directory or file path to search in' },
          literal: { type: 'boolean', description: 'Treat query as literal string (default: false)' },
          case_sensitive: { type: 'boolean', description: 'Case-sensitive search (default: false)' },
          max_results: {
            type: 'integer',
            description: `Maximum results to return (default: ${MAX_RESULTS_DEFAULT})`,
          },
        },
        required: ['query', 'path'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const query = args.query as string
      const searchPath = args.path as string
      const literal = (args.literal as boolean) ?? false
      const caseSensitive = (args.case_sensitive as boolean) ?? false
      const maxResults = (args.max_results as number) ?? MAX_RESULTS_DEFAULT

      const grepArgs: string[] = ['-rn', '--color=never']

      if (literal) grepArgs.push('-F')
      if (!caseSensitive) grepArgs.push('-i')

      grepArgs.push('-m', String(maxResults))
      grepArgs.push('--', query, searchPath)

      try {
        const { stdout } = await execFileAsync('grep', grepArgs, {
          maxBuffer: 1024 * 1024, // 1MB
          signal: ctx.signal,
        })

        const matches = parseGrepOutput(stdout, maxResults)
        return {
          ok: true,
          output: {
            query,
            path: searchPath,
            count: matches.length,
            matches,
          },
        }
      } catch (err) {
        const error = err as { code?: number; stdout?: string; message?: string }

        // grep returns exit code 1 when no matches found
        if (error.code === 1) {
          return { ok: true, output: { query, path: searchPath, count: 0, matches: [] } }
        }

        return { ok: false, error: `Search failed: ${error.message ?? String(err)}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Search for "${args.query}" in ${args.path}` }
    },
  })
}

interface GrepMatch {
  path: string
  line: number
  snippet: string
}

function parseGrepOutput(stdout: string, maxResults: number): GrepMatch[] {
  const lines = stdout.split('\n').filter(Boolean)
  const results: GrepMatch[] = []

  for (const line of lines) {
    if (results.length >= maxResults) break

    // Format: file:line:content
    const firstColon = line.indexOf(':')
    if (firstColon === -1) continue

    const secondColon = line.indexOf(':', firstColon + 1)
    if (secondColon === -1) continue

    const filePath = line.slice(0, firstColon)
    const lineNum = parseInt(line.slice(firstColon + 1, secondColon), 10)
    const snippet = line.slice(secondColon + 1)

    if (!isNaN(lineNum)) {
      results.push({ path: filePath, line: lineNum, snippet })
    }
  }

  return results
}
