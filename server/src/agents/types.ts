export interface AgentDefinition {
  /** Unique name — derived from filename if not in frontmatter */
  name: string
  /** Model identifier, e.g. "anthropic:claude-sonnet-4-6" */
  model?: string
  /** Max orchestrator turns before the agent is stopped */
  max_turns?: number
  /** System prompt — the markdown body below the frontmatter */
  system_prompt: string
  /** Short description shown in the delegate tool listing */
  description?: string
  /** If set, only these tool names are exposed to the agent's LLM */
  tools?: string[]
}
