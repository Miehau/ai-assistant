import { execFile } from 'child_process'
import { promisify } from 'util'
import type { ToolHandler, ToolResult, ToolContext } from './types.js'

const execFileAsync = promisify(execFile)

const DEFAULT_TIMEOUT_MS = 30_000
const MAX_OUTPUT_BYTES = 256 * 1024 // 256KB

export function registerShellTools(registry: { register: (h: ToolHandler) => void }): void {
  registry.register({
    metadata: {
      name: 'shell.exec',
      description: 'Execute a shell command.',
      parameters: {
        type: 'object',
        properties: {
          command: { type: 'string', description: 'Command' },
          working_dir: { type: 'string', description: 'Working directory' },
          timeout_ms: {
            type: 'integer',
            description: `Default: ${DEFAULT_TIMEOUT_MS}`,
          },
        },
        required: ['command'],
      },
      requires_approval: true,
    },
    async handle(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
      const command = args.command as string
      const workingDir = args.working_dir as string | undefined
      const timeoutMs = (args.timeout_ms as number) ?? DEFAULT_TIMEOUT_MS

      const abortController = new AbortController()

      // Link to parent signal
      const onAbort = () => abortController.abort()
      ctx.signal.addEventListener('abort', onAbort, { once: true })

      // Timeout
      const timer = setTimeout(() => abortController.abort(), timeoutMs)

      try {
        const { stdout, stderr } = await execFileAsync('/bin/sh', ['-c', command], {
          cwd: workingDir,
          maxBuffer: MAX_OUTPUT_BYTES,
          signal: abortController.signal,
        })

        return {
          ok: true,
          output: {
            stdout: truncate(stdout, MAX_OUTPUT_BYTES),
            stderr: truncate(stderr, MAX_OUTPUT_BYTES),
            exit_code: 0,
            success: true,
          },
        }
      } catch (err) {
        const error = err as {
          code?: number | string
          stdout?: string
          stderr?: string
          killed?: boolean
          message?: string
        }

        if (error.killed || error.code === 'ABORT_ERR' || ctx.signal.aborted) {
          return {
            ok: false,
            error: ctx.signal.aborted ? 'Command aborted' : 'Command timed out',
            output: {
              stdout: truncate(error.stdout ?? '', MAX_OUTPUT_BYTES),
              stderr: truncate(error.stderr ?? '', MAX_OUTPUT_BYTES),
              exit_code: -1,
              success: false,
            },
          }
        }

        // Non-zero exit code
        const exitCode = typeof error.code === 'number' ? error.code : 1
        return {
          ok: true,
          output: {
            stdout: truncate(error.stdout ?? '', MAX_OUTPUT_BYTES),
            stderr: truncate(error.stderr ?? '', MAX_OUTPUT_BYTES),
            exit_code: exitCode,
            success: false,
          },
        }
      } finally {
        clearTimeout(timer)
        ctx.signal.removeEventListener('abort', onAbort)
      }
    },
    preview(args: Record<string, unknown>): { summary: string; details?: Record<string, unknown> } {
      const cmd = String(args.command)
      const display = cmd.length > 80 ? cmd.slice(0, 77) + '...' : cmd
      return {
        summary: `Execute: ${display}`,
        details: args.working_dir ? { working_dir: args.working_dir } : undefined,
      }
    },
  })
}

function truncate(str: string, maxBytes: number): string {
  if (Buffer.byteLength(str) <= maxBytes) return str
  // Rough truncation — good enough for output capping
  const buf = Buffer.from(str)
  return buf.subarray(0, maxBytes).toString('utf-8') + '\n... [truncated]'
}
