import { execFile } from 'child_process'
import { promisify } from 'util'
import type { ToolHandler, ToolResult, ToolContext } from './types.js'
import { managedFileRefForPath, resolveManagedFilePath } from './path-policy.js'

const execFileAsync = promisify(execFile)

const MAX_RESULTS_DEFAULT = 200

export interface SearchToolOptions {
  sessionFilesRoot: string
  notesDir: string
}

export function registerSearchTools(
  registry: { register: (h: ToolHandler) => void },
  options: SearchToolOptions,
): void {
  registry.register({
    metadata: {
      name: 'web_search',
      description: 'Native web search.',
      parameters: {
        type: 'object',
        properties: {},
      },
      requires_approval: false,
    },
    async handle(): Promise<ToolResult> {
      return {
        ok: false,
        error: 'web_search is provider-native and must be consumed by the provider adapter before tool execution',
      }
    },
  })

  registry.register({
    metadata: {
      name: 'search',
      description: 'Search managed file or note content.',
      parameters: {
        type: 'object',
        properties: {
          query: { type: 'string', description: 'Pattern' },
          path: { type: 'string', description: 'Managed path' },
          literal: { type: 'boolean', description: 'Default: false' },
          case_sensitive: { type: 'boolean', description: 'Default: false' },
          max_results: {
            type: 'integer',
            description: `Default: ${MAX_RESULTS_DEFAULT}`,
          },
        },
        required: ['query', 'path'],
      },
      requires_approval: false,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const query = args.query as string
      const resolved = resolveManagedFilePath(args.path as string, {
        sessionFilesRoot: options.sessionFilesRoot,
        notesDir: options.notesDir,
        sessionId: ctx.session_id,
        access: 'read',
      })
      const searchPath = resolved.fsPath
      const literal = (args.literal as boolean) ?? false
      const caseSensitive = (args.case_sensitive as boolean) ?? false
      const maxResults = (args.max_results as number) ?? MAX_RESULTS_DEFAULT

      try {
        const matches = await runSearch(query, searchPath, {
          literal,
          caseSensitive,
          maxResults,
          signal: ctx.signal,
        })
        const logicalMatches = matches.map((match) => ({
          ...match,
          path: managedFileRefForPath(match.path, resolved),
        }))
        return {
          ok: true,
          output: {
            query,
            path: resolved.ref,
            count: logicalMatches.length,
            matches: logicalMatches,
          },
        }
      } catch (err) {
        const error = err as { code?: number; stdout?: string; message?: string }

        // grep returns exit code 1 when no matches found
        if (error.code === 1) {
          return { ok: true, output: { query, path: resolved.ref, count: 0, matches: [] } }
        }

        return { ok: false, error: `Search failed: ${error.message ?? String(err)}` }
      }
    },
    preview(args: Record<string, unknown>): { summary: string } {
      return { summary: `Search for "${args.query}" in ${args.path}` }
    },
  })
}

async function runSearch(
  query: string,
  searchPath: string,
  opts: { literal: boolean; caseSensitive: boolean; maxResults: number; signal: AbortSignal },
): Promise<GrepMatch[]> {
  const rgArgs = ['--line-number', '--no-heading', '--with-filename', '--color=never']
  if (opts.literal) rgArgs.push('--fixed-strings')
  if (!opts.caseSensitive) rgArgs.push('--ignore-case')
  rgArgs.push('--', query, searchPath)

  try {
    const { stdout } = await execFileAsync('rg', rgArgs, {
      maxBuffer: 1024 * 1024,
      signal: opts.signal,
    })
    return parseGrepOutput(stdout, opts.maxResults)
  } catch (err) {
    const error = err as { code?: number; stdout?: string; message?: string }
    if (error.code === 1) return []
    if (!String(error.message ?? '').includes('ENOENT')) throw err
  }

  const grepArgs: string[] = ['-rnH', '--color=never']
  if (opts.literal) grepArgs.push('-F')
  if (!opts.caseSensitive) grepArgs.push('-i')
  grepArgs.push('-m', String(opts.maxResults))
  grepArgs.push('--', query, searchPath)

  try {
    const { stdout } = await execFileAsync('grep', grepArgs, {
      maxBuffer: 1024 * 1024,
      signal: opts.signal,
    })
    return parseGrepOutput(stdout, opts.maxResults)
  } catch (err) {
    const error = err as { code?: number; stdout?: string; message?: string }
    if (error.code === 1) return []
    throw err
  }
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
