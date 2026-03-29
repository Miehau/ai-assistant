import fs from 'fs/promises'
import path from 'path'
import type {
  TaskFrontmatter,
  TaskRecord,
  TaskStatus,
  TaskPriority,
  CreateTaskInput,
} from './types.js'

// ---------------------------------------------------------------------------
// Frontmatter serialization (mirrors agents/loader.ts convention — no deps)
// ---------------------------------------------------------------------------

const VALID_STATUSES: TaskStatus[] = ['open', 'in_progress', 'done', 'blocked']
const VALID_PRIORITIES: TaskPriority[] = ['high', 'medium', 'low']

function asStatus(v: unknown): TaskStatus {
  return VALID_STATUSES.includes(v as TaskStatus) ? (v as TaskStatus) : 'open'
}

function asPriority(v: unknown): TaskPriority {
  return VALID_PRIORITIES.includes(v as TaskPriority) ? (v as TaskPriority) : 'medium'
}

function asStringArray(v: unknown): string[] {
  if (!Array.isArray(v)) return []
  return v.filter((x): x is string => typeof x === 'string').map((x) => x.trim()).filter(Boolean)
}

function parseYamlArray(raw: string): string[] {
  const trimmed = raw.trim()
  if (trimmed === '[]' || trimmed === '') return []
  // Handle inline array: [a, b, c]
  if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
    return trimmed.slice(1, -1).split(',').map((s) => s.trim()).filter(Boolean)
  }
  return [trimmed]
}

function serializeYamlArray(arr: string[]): string {
  if (arr.length === 0) return '[]'
  return `[${arr.join(', ')}]`
}

function normalizeFrontmatter(
  data: Record<string, unknown>,
  slug: string,
): TaskFrontmatter {
  const now = new Date().toISOString()
  return {
    id: String(data.id ?? slug),
    title: String(data.title ?? slug),
    status: asStatus(data.status),
    owner: String(data.owner ?? 'unassigned'),
    priority: asPriority(data.priority),
    depends_on: typeof data.depends_on === 'string'
      ? parseYamlArray(data.depends_on)
      : asStringArray(data.depends_on),
    output_path: data.output_path ? String(data.output_path) : undefined,
    blocked_reason: data.blocked_reason ? String(data.blocked_reason) : undefined,
    completion_note: data.completion_note ? String(data.completion_note) : undefined,
    created_by: String(data.created_by ?? 'system'),
    created_at: String(data.created_at ?? now),
    updated_at: String(data.updated_at ?? now),
    completed_at: data.completed_at ? String(data.completed_at) : undefined,
  }
}

// ---------------------------------------------------------------------------
// Parse / serialize task files
// ---------------------------------------------------------------------------

export function parseTaskFile(content: string, filePath: string): TaskRecord {
  const trimmed = content.trim()
  const slug = path.basename(filePath, '.md')

  if (!trimmed.startsWith('---')) {
    return {
      path: filePath,
      slug,
      frontmatter: normalizeFrontmatter({}, slug),
      body: trimmed,
    }
  }

  const closeIdx = trimmed.indexOf('---', 3)
  if (closeIdx === -1) {
    return {
      path: filePath,
      slug,
      frontmatter: normalizeFrontmatter({}, slug),
      body: trimmed,
    }
  }

  const fmBlock = trimmed.slice(3, closeIdx).trim()
  const body = trimmed.slice(closeIdx + 3).trim()

  const meta: Record<string, string> = {}
  for (const line of fmBlock.split('\n')) {
    const colonIdx = line.indexOf(':')
    if (colonIdx === -1) continue
    const key = line.slice(0, colonIdx).trim()
    const value = line.slice(colonIdx + 1).trim()
    if (key) meta[key] = unquoteYamlValue(value)
  }

  return {
    path: filePath,
    slug,
    frontmatter: normalizeFrontmatter(meta, slug),
    body,
  }
}

/** Quote a YAML value if it contains characters that would break the parser */
function yamlValue(v: string): string {
  if (v.includes(':') || v.includes('#') || v.includes('"') || v.startsWith("'")) {
    return `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`
  }
  return v
}

/** Strip surrounding quotes from a parsed YAML value */
function unquoteYamlValue(v: string): string {
  if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) {
    return v.slice(1, -1).replace(/\\"/g, '"').replace(/\\\\/g, '\\')
  }
  return v
}

function serializeTask(task: TaskRecord): string {
  const fm = task.frontmatter
  const lines: string[] = ['---']

  lines.push(`id: ${fm.id}`)
  lines.push(`title: ${yamlValue(fm.title)}`)
  lines.push(`status: ${fm.status}`)
  lines.push(`owner: ${fm.owner}`)
  lines.push(`priority: ${fm.priority}`)
  lines.push(`depends_on: ${serializeYamlArray(fm.depends_on)}`)
  if (fm.output_path) lines.push(`output_path: ${yamlValue(fm.output_path)}`)
  if (fm.blocked_reason) lines.push(`blocked_reason: ${yamlValue(fm.blocked_reason)}`)
  if (fm.completion_note) lines.push(`completion_note: ${yamlValue(fm.completion_note)}`)
  lines.push(`created_by: ${fm.created_by}`)
  lines.push(`created_at: ${fm.created_at}`)
  lines.push(`updated_at: ${fm.updated_at}`)
  if (fm.completed_at) lines.push(`completed_at: ${fm.completed_at}`)

  lines.push('---')
  lines.push('')
  lines.push(task.body)
  lines.push('')

  return lines.join('\n')
}

// ---------------------------------------------------------------------------
// Filesystem operations
// ---------------------------------------------------------------------------

async function ensureDir(dir: string): Promise<void> {
  await fs.mkdir(dir, { recursive: true })
}

async function readTask(filePath: string): Promise<TaskRecord> {
  const content = await fs.readFile(filePath, 'utf-8')
  return parseTaskFile(content, filePath)
}

async function writeTask(task: TaskRecord): Promise<void> {
  await ensureDir(path.dirname(task.path))
  await fs.writeFile(task.path, serializeTask(task), 'utf-8')
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function listTasks(tasksDir: string): Promise<TaskRecord[]> {
  await ensureDir(tasksDir)
  const files = (await fs.readdir(tasksDir)).filter((f) => f.endsWith('.md'))
  const tasks = await Promise.all(
    files.map((f) => readTask(path.join(tasksDir, f))),
  )
  return tasks.sort((a, b) => a.slug.localeCompare(b.slug))
}

export async function findTaskById(
  tasksDir: string,
  id: string,
): Promise<TaskRecord | null> {
  const tasks = await listTasks(tasksDir)
  return tasks.find((t) => t.frontmatter.id === id) ?? null
}

export async function createTask(
  tasksDir: string,
  input: CreateTaskInput,
): Promise<TaskRecord> {
  const slug = input.id
  const filename = `${slug}.md`
  const filePath = path.join(tasksDir, filename)

  const now = new Date().toISOString()
  const task: TaskRecord = {
    path: filePath,
    slug,
    frontmatter: {
      id: input.id,
      title: input.title,
      status: 'open',
      owner: input.owner,
      priority: input.priority,
      depends_on: input.dependsOn ?? [],
      output_path: input.outputPath,
      created_by: input.createdBy,
      created_at: now,
      updated_at: now,
    },
    body: input.body,
  }

  await writeTask(task)
  return task
}

export async function updateTask(
  tasksDir: string,
  id: string,
  patch: {
    status?: TaskStatus
    body?: string
    blocked_reason?: string
    completion_note?: string
  },
): Promise<TaskRecord> {
  const task = await findTaskById(tasksDir, id)
  if (!task) throw new Error(`Task not found: ${id}`)

  if (patch.status) task.frontmatter.status = patch.status
  if (patch.body !== undefined) task.body = patch.body
  if (patch.blocked_reason !== undefined) task.frontmatter.blocked_reason = patch.blocked_reason
  if (patch.completion_note !== undefined) task.frontmatter.completion_note = patch.completion_note

  task.frontmatter.updated_at = new Date().toISOString()

  if (patch.status === 'done') {
    task.frontmatter.completed_at = task.frontmatter.updated_at
  }

  await writeTask(task)
  return task
}
