import { randomUUID } from 'node:crypto'
import type { ToolHandler, ToolResult } from './types.js'
import type { TaskPriority, TaskStatus, CreateTaskInput } from '../tasks/types.js'
import { listTasks, createTask, updateTask } from '../tasks/storage.js'

const VALID_PRIORITIES: TaskPriority[] = ['high', 'medium', 'low']
const VALID_STATUSES: TaskStatus[] = ['open', 'in_progress', 'done', 'blocked']

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
}

function makeTaskId(title: string): string {
  return `${slugify(title)}-${randomUUID().slice(0, 6)}`
}

export function registerTaskTools(
  registry: { register: (h: ToolHandler) => void },
  tasksDir: string,
  workspaceDir: string,
): void {
  // -------------------------------------------------------------------------
  // tasks.create — create a single task file
  // -------------------------------------------------------------------------
  registry.register({
    metadata: {
      name: 'tasks.create',
      description:
        'Create a new task as a Markdown file on disk. Returns the task ID and file path. ' +
        'Use this to plan subtasks before delegating them to agents.',
      parameters: {
        type: 'object',
        properties: {
          title: { type: 'string', description: 'Short human-readable task title' },
          owner: {
            type: 'string',
            description: 'Name of the agent responsible (must match an agent definition name)',
          },
          priority: {
            type: 'string',
            enum: VALID_PRIORITIES,
            description: 'Task priority. Defaults to medium.',
          },
          depends_on: {
            type: 'array',
            items: { type: 'string' },
            description: 'IDs of tasks that must complete before this one',
          },
          output_path: {
            type: 'string',
            description: 'Path where the task output should be written (optional)',
          },
          body: {
            type: 'string',
            description: 'Full task description with steps, success criteria, and expected output format',
          },
        },
        required: ['title', 'owner', 'body'],
      },
      requires_approval: false,
    },
    async handle(args): Promise<ToolResult> {
      const title = String(args.title ?? '').trim()
      const owner = String(args.owner ?? '').trim()
      const body = String(args.body ?? '').trim()

      if (!title || !owner || !body) {
        return { ok: false, error: 'title, owner, and body are required' }
      }

      const priority = VALID_PRIORITIES.includes(args.priority as TaskPriority)
        ? (args.priority as TaskPriority)
        : 'medium'

      const dependsOn = Array.isArray(args.depends_on)
        ? (args.depends_on as unknown[]).filter((x): x is string => typeof x === 'string')
        : []

      const id = makeTaskId(title)
      const input: CreateTaskInput = {
        id,
        title,
        owner,
        priority,
        dependsOn,
        outputPath: args.output_path ? String(args.output_path) : undefined,
        body,
        createdBy: 'agent',
      }

      const task = await createTask(tasksDir, input)
      return {
        ok: true,
        output: {
          id: task.frontmatter.id,
          path: task.path,
          status: task.frontmatter.status,
          workspace_dir: workspaceDir,
        },
      }
    },
    preview(args) {
      return { summary: `Create task: ${args.title}` }
    },
  })

  // -------------------------------------------------------------------------
  // tasks.create_batch — create multiple tasks at once
  // -------------------------------------------------------------------------
  registry.register({
    metadata: {
      name: 'tasks.create_batch',
      description:
        'Create multiple tasks at once. Use for initial plan decomposition. ' +
        'All tasks are validated before any are written — if one fails validation, none are created.',
      parameters: {
        type: 'object',
        properties: {
          tasks: {
            type: 'array',
            items: {
              type: 'object',
              properties: {
                title: { type: 'string' },
                owner: { type: 'string' },
                priority: { type: 'string', enum: VALID_PRIORITIES },
                depends_on: { type: 'array', items: { type: 'string' } },
                output_path: { type: 'string' },
                body: { type: 'string' },
              },
              required: ['title', 'owner', 'body'],
            },
            description: 'Array of task definitions to create',
          },
        },
        required: ['tasks'],
      },
      requires_approval: false,
    },
    async handle(args): Promise<ToolResult> {
      const rawTasks = args.tasks
      if (!Array.isArray(rawTasks) || rawTasks.length === 0) {
        return { ok: false, error: 'tasks must be a non-empty array' }
      }

      // Validate all first
      const inputs: CreateTaskInput[] = []
      for (let i = 0; i < rawTasks.length; i++) {
        const t = rawTasks[i] as Record<string, unknown>
        const title = String(t.title ?? '').trim()
        const owner = String(t.owner ?? '').trim()
        const body = String(t.body ?? '').trim()

        if (!title || !owner || !body) {
          return { ok: false, error: `Task at index ${i} is missing title, owner, or body` }
        }

        const priority = VALID_PRIORITIES.includes(t.priority as TaskPriority)
          ? (t.priority as TaskPriority)
          : 'medium'

        const dependsOn = Array.isArray(t.depends_on)
          ? (t.depends_on as unknown[]).filter((x): x is string => typeof x === 'string')
          : []

        inputs.push({
          id: makeTaskId(title),
          title,
          owner,
          priority,
          dependsOn,
          outputPath: t.output_path ? String(t.output_path) : undefined,
          body,
          createdBy: 'agent',
        })
      }

      // Write all — surface partial failure if I/O breaks mid-batch
      const created = []
      try {
        for (const input of inputs) {
          const task = await createTask(tasksDir, input)
          created.push({
            id: task.frontmatter.id,
            title: task.frontmatter.title,
            owner: task.frontmatter.owner,
            path: task.path,
          })
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        return {
          ok: false,
          error: `Failed after creating ${created.length}/${inputs.length} tasks: ${msg}`,
          output: created.length > 0 ? { partial: created } : undefined,
        }
      }

      return {
        ok: true,
        output: { count: created.length, tasks: created, workspace_dir: workspaceDir },
      }
    },
    preview(args) {
      const count = Array.isArray(args.tasks) ? args.tasks.length : 0
      return { summary: `Create ${count} tasks` }
    },
  })

  // -------------------------------------------------------------------------
  // tasks.list — list tasks with optional status filter
  // -------------------------------------------------------------------------
  registry.register({
    metadata: {
      name: 'tasks.list',
      description:
        'List all tasks with their status, owner, priority, and path. ' +
        'Optionally filter by status. Returns a summary — use files.read to see full task body.',
      parameters: {
        type: 'object',
        properties: {
          status: {
            type: 'string',
            enum: [...VALID_STATUSES, 'all'],
            description: 'Filter by status. Defaults to "all".',
          },
        },
        required: [],
      },
      requires_approval: false,
    },
    async handle(args): Promise<ToolResult> {
      const filter = String(args.status ?? 'all')
      const tasks = await listTasks(tasksDir)
      const filtered = filter === 'all'
        ? tasks
        : tasks.filter((t) => t.frontmatter.status === filter)

      const summary = filtered.map((t) => ({
        id: t.frontmatter.id,
        title: t.frontmatter.title,
        status: t.frontmatter.status,
        owner: t.frontmatter.owner,
        priority: t.frontmatter.priority,
        depends_on: t.frontmatter.depends_on,
        path: t.path,
        ...(t.frontmatter.completion_note ? { completion_note: t.frontmatter.completion_note } : {}),
        ...(t.frontmatter.blocked_reason ? { blocked_reason: t.frontmatter.blocked_reason } : {}),
      }))

      // Also include status counts
      const counts: Record<string, number> = { open: 0, in_progress: 0, done: 0, blocked: 0 }
      for (const t of tasks) {
        counts[t.frontmatter.status] = (counts[t.frontmatter.status] ?? 0) + 1
      }

      return {
        ok: true,
        output: { total: tasks.length, counts, tasks: summary, workspace_dir: workspaceDir },
      }
    },
    preview() {
      return { summary: 'List tasks' }
    },
  })

  // -------------------------------------------------------------------------
  // tasks.update — update status, body, or notes on a task
  // -------------------------------------------------------------------------
  registry.register({
    metadata: {
      name: 'tasks.update',
      description:
        'Update a task\'s status, body, or notes. Use after a delegate call returns to mark the task done or blocked.',
      parameters: {
        type: 'object',
        properties: {
          id: { type: 'string', description: 'Task ID (from tasks.create output)' },
          status: {
            type: 'string',
            enum: VALID_STATUSES,
            description: 'New status for the task',
          },
          completion_note: {
            type: 'string',
            description: 'Summary of what was accomplished (set when marking done)',
          },
          blocked_reason: {
            type: 'string',
            description: 'Why the task is blocked (set when marking blocked)',
          },
          body: {
            type: 'string',
            description: 'Replace the task body (optional, use for plan revisions)',
          },
        },
        required: ['id'],
      },
      requires_approval: false,
    },
    async handle(args): Promise<ToolResult> {
      const id = String(args.id ?? '').trim()
      if (!id) return { ok: false, error: 'id is required' }

      const status = VALID_STATUSES.includes(args.status as TaskStatus)
        ? (args.status as TaskStatus)
        : undefined

      try {
        const task = await updateTask(tasksDir, id, {
          status,
          body: args.body !== undefined ? String(args.body) : undefined,
          completion_note: args.completion_note !== undefined ? String(args.completion_note) : undefined,
          blocked_reason: args.blocked_reason !== undefined ? String(args.blocked_reason) : undefined,
        })
        return {
          ok: true,
          output: {
            id: task.frontmatter.id,
            status: task.frontmatter.status,
            path: task.path,
          },
        }
      } catch (err) {
        return {
          ok: false,
          error: err instanceof Error ? err.message : String(err),
        }
      }
    },
    preview(args) {
      return { summary: `Update task ${args.id}: status=${args.status ?? 'unchanged'}` }
    },
  })
}
