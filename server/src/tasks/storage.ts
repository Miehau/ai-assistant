import fs from 'fs/promises'
import path from 'path'
import type {
  TaskFrontmatter,
  TaskKind,
  TaskOutputProfile,
  TaskRecord,
  TaskStatus,
  TaskPriority,
  CreateTaskInput,
} from './types.js'

// ---------------------------------------------------------------------------
// Frontmatter serialization (mirrors agents/loader.ts convention — no deps)
// ---------------------------------------------------------------------------

const VALID_STATUSES: TaskStatus[] = [
  'open',
  'in_progress',
  'queued',
  'running',
  'callback_pending',
  'done',
  'blocked',
  'failed',
  'cancelled',
]
const VALID_PRIORITIES: TaskPriority[] = ['high', 'medium', 'low']
const VALID_KINDS: TaskKind[] = ['planning', 'background']
const VALID_OUTPUT_PROFILES: TaskOutputProfile[] = ['generic', 'research']

function asStatus(v: unknown): TaskStatus {
  return VALID_STATUSES.includes(v as TaskStatus) ? (v as TaskStatus) : 'open'
}

function asPriority(v: unknown): TaskPriority {
  return VALID_PRIORITIES.includes(v as TaskPriority) ? (v as TaskPriority) : 'medium'
}

function asKind(v: unknown): TaskKind | undefined {
  return VALID_KINDS.includes(v as TaskKind) ? (v as TaskKind) : undefined
}

function asOutputProfile(v: unknown): TaskOutputProfile | undefined {
  return VALID_OUTPUT_PROFILES.includes(v as TaskOutputProfile) ? (v as TaskOutputProfile) : undefined
}

function asOptionalString(v: unknown): string | undefined {
  if (v === undefined || v === null || v === '') return undefined
  return String(v)
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
    kind: asKind(data.kind),
    owner: String(data.owner ?? 'unassigned'),
    priority: asPriority(data.priority),
    depends_on: typeof data.depends_on === 'string'
      ? parseYamlArray(data.depends_on)
      : asStringArray(data.depends_on),
    output_path: asOptionalString(data.output_path),
    callback_agent_id: asOptionalString(data.callback_agent_id),
    callback_session_id: asOptionalString(data.callback_session_id),
    execution_session_id: asOptionalString(data.execution_session_id),
    execution_agent_id: asOptionalString(data.execution_agent_id),
    output_note: asOptionalString(data.output_note),
    output_artifact: asOptionalString(data.output_artifact),
    output_profile: asOutputProfile(data.output_profile),
    error: asOptionalString(data.error),
    telegram_connection_id: asOptionalString(data.telegram_connection_id),
    telegram_chat_id: asOptionalString(data.telegram_chat_id),
    telegram_original_message_id: asOptionalString(data.telegram_original_message_id),
    telegram_accepted_message_id: asOptionalString(data.telegram_accepted_message_id),
    telegram_completion_message_id: asOptionalString(data.telegram_completion_message_id),
    blocked_reason: asOptionalString(data.blocked_reason),
    completion_note: asOptionalString(data.completion_note),
    created_by: String(data.created_by ?? 'system'),
    created_at: String(data.created_at ?? now),
    updated_at: String(data.updated_at ?? now),
    completed_at: asOptionalString(data.completed_at),
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
  if (fm.kind) lines.push(`kind: ${fm.kind}`)
  lines.push(`owner: ${fm.owner}`)
  lines.push(`priority: ${fm.priority}`)
  lines.push(`depends_on: ${serializeYamlArray(fm.depends_on)}`)
  if (fm.output_path) lines.push(`output_path: ${yamlValue(fm.output_path)}`)
  if (fm.callback_agent_id) lines.push(`callback_agent_id: ${yamlValue(fm.callback_agent_id)}`)
  if (fm.callback_session_id) lines.push(`callback_session_id: ${yamlValue(fm.callback_session_id)}`)
  if (fm.execution_session_id) lines.push(`execution_session_id: ${yamlValue(fm.execution_session_id)}`)
  if (fm.execution_agent_id) lines.push(`execution_agent_id: ${yamlValue(fm.execution_agent_id)}`)
  if (fm.output_note) lines.push(`output_note: ${yamlValue(fm.output_note)}`)
  if (fm.output_artifact) lines.push(`output_artifact: ${yamlValue(fm.output_artifact)}`)
  if (fm.output_profile) lines.push(`output_profile: ${fm.output_profile}`)
  if (fm.error) lines.push(`error: ${yamlValue(fm.error)}`)
  if (fm.telegram_connection_id) lines.push(`telegram_connection_id: ${yamlValue(fm.telegram_connection_id)}`)
  if (fm.telegram_chat_id) lines.push(`telegram_chat_id: ${yamlValue(fm.telegram_chat_id)}`)
  if (fm.telegram_original_message_id) lines.push(`telegram_original_message_id: ${yamlValue(fm.telegram_original_message_id)}`)
  if (fm.telegram_accepted_message_id) lines.push(`telegram_accepted_message_id: ${yamlValue(fm.telegram_accepted_message_id)}`)
  if (fm.telegram_completion_message_id) lines.push(`telegram_completion_message_id: ${yamlValue(fm.telegram_completion_message_id)}`)
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
      status: input.status ?? 'open',
      kind: input.kind,
      owner: input.owner,
      priority: input.priority,
      depends_on: input.dependsOn ?? [],
      output_path: input.outputPath,
      output_profile: input.outputProfile,
      callback_agent_id: input.callbackAgentId,
      callback_session_id: input.callbackSessionId,
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
    error?: string
    execution_session_id?: string
    execution_agent_id?: string
    output_note?: string
    output_artifact?: string
    telegram_connection_id?: string
    telegram_chat_id?: string
    telegram_original_message_id?: string
    telegram_accepted_message_id?: string
    telegram_completion_message_id?: string
  },
): Promise<TaskRecord> {
  const task = await findTaskById(tasksDir, id)
  if (!task) throw new Error(`Task not found: ${id}`)

  if (patch.status) task.frontmatter.status = patch.status
  if (patch.body !== undefined) task.body = patch.body
  if (patch.blocked_reason !== undefined) task.frontmatter.blocked_reason = patch.blocked_reason
  if (patch.completion_note !== undefined) task.frontmatter.completion_note = patch.completion_note
  if (patch.error !== undefined) task.frontmatter.error = patch.error
  if (patch.execution_session_id !== undefined) task.frontmatter.execution_session_id = patch.execution_session_id
  if (patch.execution_agent_id !== undefined) task.frontmatter.execution_agent_id = patch.execution_agent_id
  if (patch.output_note !== undefined) task.frontmatter.output_note = patch.output_note
  if (patch.output_artifact !== undefined) task.frontmatter.output_artifact = patch.output_artifact
  if (patch.telegram_connection_id !== undefined) task.frontmatter.telegram_connection_id = patch.telegram_connection_id
  if (patch.telegram_chat_id !== undefined) task.frontmatter.telegram_chat_id = patch.telegram_chat_id
  if (patch.telegram_original_message_id !== undefined) task.frontmatter.telegram_original_message_id = patch.telegram_original_message_id
  if (patch.telegram_accepted_message_id !== undefined) task.frontmatter.telegram_accepted_message_id = patch.telegram_accepted_message_id
  if (patch.telegram_completion_message_id !== undefined) task.frontmatter.telegram_completion_message_id = patch.telegram_completion_message_id

  task.frontmatter.updated_at = new Date().toISOString()

  if (patch.status === 'done' || patch.status === 'failed' || patch.status === 'cancelled') {
    task.frontmatter.completed_at = task.frontmatter.updated_at
  }

  await writeTask(task)
  return task
}
