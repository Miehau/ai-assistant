export type TaskStatus = 'open' | 'in_progress' | 'done' | 'blocked'
export type TaskPriority = 'high' | 'medium' | 'low'

export interface TaskFrontmatter {
  id: string
  title: string
  status: TaskStatus
  owner: string
  priority: TaskPriority
  depends_on: string[]
  output_path?: string
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
  dependsOn?: string[]
  outputPath?: string
  body: string
  createdBy: string
}
