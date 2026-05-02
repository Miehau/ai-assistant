import fs from 'fs/promises'
import path from 'path'
import type { RuntimeContext } from '../lib/runtime.js'
import { logger } from '../lib/logger.js'
import { buildDeps, prepareSessionTurn } from '../services/session-runner.js'
import { runAgent } from '../orchestrator/runner.js'
import { getSessionFilesDir } from '../tools/path-policy.js'
import { listTasks, updateTask } from './storage.js'
import type { TaskRecord } from './types.js'
import { EVENT_TYPES } from '../events/types.js'

interface TaskRunnerOptions {
  tasksDir: string
  notesDir: string
  intervalMs?: number
}

const NOTE_REF_RE = /@note\/[^\s)\]>"']+/i
const URL_RE = /https?:\/\/[^\s)\]>"']+/i
const PLACEHOLDER_RE = /\bturn\d+(?:search|fetch|open|view)\d+\b/i
const PRIVATE_CITATION_RE = /[\uE000-\uF8FF]/
const ARTIFACT_RE = /artifact:\/\//i

export class TaskRunner {
  private running = false
  private processing = false
  private timer: NodeJS.Timeout | null = null
  private readonly intervalMs: number

  constructor(
    private readonly runtime: RuntimeContext,
    private readonly options: TaskRunnerOptions,
  ) {
    this.intervalMs = options.intervalMs ?? 2000
  }

  async start(): Promise<void> {
    if (this.running) return
    this.running = true
    await this.recoverInterruptedTasks()
    this.schedule(this.intervalMs)
  }

  stop(): void {
    this.running = false
    if (this.timer) {
      clearTimeout(this.timer)
      this.timer = null
    }
  }

  wake(): void {
    if (!this.running) return
    this.schedule(0)
  }

  private schedule(delayMs: number): void {
    if (this.timer) clearTimeout(this.timer)
    this.timer = setTimeout(() => {
      void this.tick()
    }, delayMs)
  }

  private async tick(): Promise<void> {
    if (!this.running || this.processing) return
    this.processing = true
    try {
      await this.processOne()
    } catch (err) {
      logger.error({ err }, 'Task runner tick failed')
    } finally {
      this.processing = false
      if (this.running) this.schedule(this.intervalMs)
    }
  }

  private async recoverInterruptedTasks(): Promise<void> {
    const tasks = await listTasks(this.options.tasksDir)
    for (const task of tasks) {
      if (task.frontmatter.kind !== 'background') continue
      if (task.frontmatter.status !== 'running') continue
      await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'failed',
        error: 'Server restarted while task was running',
        completion_note: 'Task was interrupted by server restart and was not resumed automatically.',
      })
    }
  }

  private async processOne(): Promise<void> {
    const tasks = await listTasks(this.options.tasksDir)
    const callbackTask = tasks.find((task) => (
      task.frontmatter.kind === 'background' &&
      task.frontmatter.status === 'callback_pending'
    ))
    if (callbackTask) {
      await this.processCallback(callbackTask)
      return
    }

    const ready = tasks.find((task) => (
      task.frontmatter.kind === 'background' &&
      task.frontmatter.status === 'queued' &&
      this.dependenciesSatisfied(task, tasks)
    ))
    if (ready) {
      await this.executeTask(ready)
    }
  }

  private dependenciesSatisfied(task: TaskRecord, tasks: TaskRecord[]): boolean {
    if (task.frontmatter.depends_on.length === 0) return true
    const byId = new Map(tasks.map((candidate) => [candidate.frontmatter.id, candidate]))
    return task.frontmatter.depends_on.every((id) => byId.get(id)?.frontmatter.status === 'done')
  }

  private async executeTask(task: TaskRecord): Promise<void> {
    const owner = task.frontmatter.owner
    const agentDef = this.runtime.agentDefinitions.get(owner)
    if (!agentDef) {
      const failedTask = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'failed',
        error: `Unknown owner agent: ${owner}`,
      })
      this.emitTaskEvent(EVENT_TYPES.TASK_FAILED, failedTask)
      return
    }

    const runningTask = await updateTask(this.options.tasksDir, task.frontmatter.id, { status: 'running', error: '' })
    this.emitTaskEvent(EVENT_TYPES.TASK_RUNNING, runningTask)

    try {
      const callbackSession = task.frontmatter.callback_session_id
        ? await this.runtime.repositories.sessions.getById(task.frontmatter.callback_session_id)
        : null
      if (!callbackSession) {
        throw new Error(`Callback session not found: ${task.frontmatter.callback_session_id ?? '(missing)'}`)
      }

      const prepared = await prepareSessionTurn(this.runtime, {
        userId: callbackSession.userId,
        agent: owner,
        input: task.body,
        instructions: [
          `Background task ID: ${task.frontmatter.id}`,
          'Complete the assigned task. Return the final result as markdown.',
          'If you save a durable note yourself, include the @note/... path in the final response.',
        ].join('\n'),
      })

      await updateTask(this.options.tasksDir, task.frontmatter.id, {
        execution_session_id: prepared.sessionId,
        execution_agent_id: prepared.agent.id,
      })

      const deps = buildDeps(this.runtime, prepared.model)
      const ac = new AbortController()
      this.runtime.agentAbortControllers.set(prepared.agent.id, ac)
      let result: Awaited<ReturnType<typeof runAgent>>
      try {
        result = await runAgent(prepared.agent.id, deps, {
          signal: AbortSignal.any([this.runtime.shutdownController.signal, ac.signal]),
        })
      } finally {
        this.runtime.agentAbortControllers.delete(prepared.agent.id)
      }

      if (result.status !== 'completed') {
        throw new Error(result.error ?? `Task owner agent finished with status: ${result.status}`)
      }

      const output = result.result ?? ''
      const outputArtifact = await this.writeArtifact(prepared.sessionId, task.frontmatter.id, output)
      const outputNote = await this.ensureNote(task, output)
      const summary = summarizeForFrontmatter(output, outputNote)

      const callbackPendingTask = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'callback_pending',
        output_artifact: outputArtifact,
        output_note: outputNote,
        completion_note: summary,
      })
      this.emitTaskEvent(EVENT_TYPES.TASK_CALLBACK_PENDING, callbackPendingTask)
      this.wake()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      const failedTask = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'failed',
        error: message,
        completion_note: `Task failed: ${message}`,
      })
      this.emitTaskEvent(EVENT_TYPES.TASK_FAILED, failedTask)
    }
  }

  private async processCallback(task: TaskRecord): Promise<void> {
    const callbackAgentId = task.frontmatter.callback_agent_id
    if (!callbackAgentId) {
      const completedTask = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'done',
        completion_note: task.frontmatter.completion_note ?? 'Task completed without callback agent metadata.',
      })
      this.emitTaskEvent(EVENT_TYPES.TASK_COMPLETED, completedTask)
      return
    }

    const agent = await this.runtime.repositories.agents.getById(callbackAgentId)
    if (!agent) {
      const failedTask = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'failed',
        error: `Callback agent not found: ${callbackAgentId}`,
      })
      this.emitTaskEvent(EVENT_TYPES.TASK_FAILED, failedTask)
      return
    }
    if (agent.status === 'running' || agent.status === 'waiting' || agent.status === 'pending') {
      return
    }

    const callbackMessage = this.buildCallbackMessage(task)
    await this.runtime.repositories.items.create({
      agentId: agent.id,
      type: 'message',
      role: 'user',
      content: callbackMessage,
      turnNumber: agent.turnCount,
    })
    await this.runtime.repositories.agents.update(agent.id, {
      status: 'running',
      result: null,
      error: null,
      completedAt: null,
    })

    const deps = buildDeps(this.runtime, agent.config.model)
    const ac = new AbortController()
    this.runtime.agentAbortControllers.set(agent.id, ac)
    try {
      const result = await runAgent(agent.id, deps, {
        signal: AbortSignal.any([this.runtime.shutdownController.signal, ac.signal]),
      })
      if (result.status !== 'completed') {
        throw new Error(result.error ?? `Callback agent finished with status: ${result.status}`)
      }
      const completedTask = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'done',
        completion_note: `Callback delivered. ${task.frontmatter.completion_note ?? ''}`.trim(),
      })
      this.emitTaskEvent(EVENT_TYPES.TASK_COMPLETED, completedTask)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      const failedTask = await updateTask(this.options.tasksDir, task.frontmatter.id, {
        status: 'failed',
        error: `Callback failed: ${message}`,
      })
      this.emitTaskEvent(EVENT_TYPES.TASK_FAILED, failedTask)
    } finally {
      this.runtime.agentAbortControllers.delete(agent.id)
    }
  }

  private buildCallbackMessage(task: TaskRecord): string {
    return [
      'Background task completed.',
      '',
      `Task ID: ${task.frontmatter.id}`,
      `Title: ${task.frontmatter.title}`,
      `Status: ${task.frontmatter.status}`,
      `Owner: ${task.frontmatter.owner}`,
      task.frontmatter.output_note ? `Output note: ${task.frontmatter.output_note}` : undefined,
      task.frontmatter.output_artifact ? `Output artifact: ${task.frontmatter.output_artifact}` : undefined,
      task.frontmatter.completion_note ? `Summary: ${task.frontmatter.completion_note}` : undefined,
      '',
      'Decide what to tell the user. Keep transport-facing responses concise when this task came from an external transport.',
    ].filter(Boolean).join('\n')
  }

  private async ensureNote(task: TaskRecord, output: string): Promise<string> {
    const existing = output.match(NOTE_REF_RE)?.[0]
    if (existing) return existing

    const profile = task.frontmatter.output_profile ?? 'generic'
    const validationErrors = validateNoteMarkdown(output, profile)
    if (validationErrors.length > 0) {
      throw new Error(`Task output failed ${profile} note validation: ${validationErrors.join('; ')}`)
    }

    const filename = `${sanitizeFilename(task.frontmatter.title || task.frontmatter.id)}.md`
    const ref = `@note/${filename}`
    const outputPath = path.join(this.options.notesDir, filename)
    await fs.mkdir(path.dirname(outputPath), { recursive: true })
    await fs.writeFile(outputPath, output.endsWith('\n') ? output : `${output}\n`, 'utf-8')
    return ref
  }

  private async writeArtifact(sessionId: string, taskId: string, output: string): Promise<string> {
    const dir = path.join(getSessionFilesDir(this.runtime.sessionFilesRoot, sessionId), 'artifacts', 'task-runner')
    await fs.mkdir(dir, { recursive: true })
    const filename = `${safePathPart(taskId)}-result.md`
    await fs.writeFile(path.join(dir, filename), output.endsWith('\n') ? output : `${output}\n`, 'utf-8')
    return `artifact://task-runner/${filename}`
  }

  private emitTaskEvent(
    type:
      | typeof EVENT_TYPES.TASK_RUNNING
      | typeof EVENT_TYPES.TASK_CALLBACK_PENDING
      | typeof EVENT_TYPES.TASK_COMPLETED
      | typeof EVENT_TYPES.TASK_FAILED,
    task: TaskRecord,
  ): void {
    this.runtime.events.emit({
      type,
      agent_id: task.frontmatter.callback_agent_id ?? task.frontmatter.execution_agent_id ?? 'task-runner',
      session_id: task.frontmatter.callback_session_id ?? task.frontmatter.execution_session_id ?? 'tasks',
      payload: {
        taskId: task.frontmatter.id,
        title: task.frontmatter.title,
        status: task.frontmatter.status,
        callbackAgentId: task.frontmatter.callback_agent_id,
        callbackSessionId: task.frontmatter.callback_session_id,
        outputNote: task.frontmatter.output_note,
        outputArtifact: task.frontmatter.output_artifact,
        ...(task.frontmatter.error ? { error: task.frontmatter.error } : {}),
      },
      timestamp: Date.now(),
    })
  }
}

function validateNoteMarkdown(markdown: string, profile: 'generic' | 'research'): string[] {
  const errors: string[] = []
  if (!markdown.trim()) errors.push('output is empty')
  if (PLACEHOLDER_RE.test(markdown)) errors.push('replace provider placeholder citations with raw source URLs')
  if (PRIVATE_CITATION_RE.test(markdown)) errors.push('remove private citation markers')
  if (ARTIFACT_RE.test(markdown)) errors.push('resolve artifact references before saving the final note')
  if (profile === 'research' && !URL_RE.test(markdown)) errors.push('include at least one raw http(s) source URL')
  return errors
}

function summarizeForFrontmatter(output: string, noteRef: string): string {
  const firstParagraph = output
    .split(/\n\s*\n/)
    .map((part) => part.trim())
    .find(Boolean)
    ?.replace(/\s+/g, ' ')
    .slice(0, 240)
  return [firstParagraph, `Full note: ${noteRef}`].filter(Boolean).join(' ')
}

function sanitizeFilename(title: string): string {
  const sanitized = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80)
  return sanitized || `task-note-${Date.now()}`
}

function safePathPart(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 96) || 'task'
}
