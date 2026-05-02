export type TaskStatus =
  | 'open'
  | 'in_progress'
  | 'queued'
  | 'running'
  | 'callback_pending'
  | 'done'
  | 'blocked'
  | 'failed'
  | 'cancelled'
export type TaskPriority = 'high' | 'medium' | 'low'
export type TaskKind = 'planning' | 'background'
export type TaskOutputProfile = 'generic' | 'research'

export interface TaskFrontmatter {
  id: string
  title: string
  status: TaskStatus
  kind?: TaskKind
  owner: string
  priority: TaskPriority
  depends_on: string[]
  output_path?: string
  callback_agent_id?: string
  callback_session_id?: string
  execution_session_id?: string
  execution_agent_id?: string
  output_note?: string
  output_artifact?: string
  output_profile?: TaskOutputProfile
  error?: string
  telegram_connection_id?: string
  telegram_chat_id?: string
  telegram_original_message_id?: string
  telegram_accepted_message_id?: string
  telegram_completion_message_id?: string
  blocked_reason?: string
  completion_note?: string
  created_by: string
  created_at: string
  updated_at: string
  completed_at?: string
}

export interface TaskRecord {
  /** Absolute path to the .md file */
  path: string
  /** Filename without extension */
  slug: string
  frontmatter: TaskFrontmatter
  /** Markdown body — task description, steps, success criteria */
  body: string
}

export interface CreateTaskInput {
  id: string
  title: string
  owner: string
  priority: TaskPriority
  status?: TaskStatus
  kind?: TaskKind
  dependsOn?: string[]
  outputPath?: string
  outputProfile?: TaskOutputProfile
  callbackAgentId?: string
  callbackSessionId?: string
  body: string
  createdBy: string
}
