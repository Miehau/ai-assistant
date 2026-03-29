import fs from 'fs/promises'
import path from 'path'
import type { AgentDefinition } from './types.js'

/**
 * Parse a markdown file with YAML-like frontmatter into an AgentDefinition.
 *
 * Format:
 * ```
 * ---
 * name: researcher
 * model: anthropic:claude-sonnet-4-6
 * max_turns: 30
 * description: A focused research agent
 * ---
 * You are a research agent. Your job is to...
 * ```
 *
 * The body below the closing `---` becomes the system_prompt.
 * Filename (without extension) is the fallback for `name`.
 */
export function parseAgentFile(content: string, filename: string): AgentDefinition {
  const trimmed = content.trim()
  const fallbackName = path.basename(filename, path.extname(filename))

  if (!trimmed.startsWith('---')) {
    return { name: fallbackName, system_prompt: trimmed }
  }

  const closeIdx = trimmed.indexOf('---', 3)
  if (closeIdx === -1) {
    throw new Error(`Invalid frontmatter in ${filename}: missing closing ---`)
  }

  const frontmatterBlock = trimmed.slice(3, closeIdx).trim()
  const body = trimmed.slice(closeIdx + 3).trim()

  const meta: Record<string, string> = {}
  for (const line of frontmatterBlock.split('\n')) {
    const colonIdx = line.indexOf(':')
    if (colonIdx === -1) continue
    const key = line.slice(0, colonIdx).trim()
    const value = line.slice(colonIdx + 1).trim()
    if (key && value) meta[key] = value
  }

  const tools = meta.tools
    ? meta.tools.split(',').map((t) => t.trim()).filter(Boolean)
    : undefined

  return {
    name: meta.name ?? fallbackName,
    model: meta.model,
    max_turns: meta.max_turns ? parseInt(meta.max_turns, 10) : undefined,
    description: meta.description,
    system_prompt: body,
    tools,
  }
}

/**
 * Load all `.md` files from a directory and return parsed AgentDefinitions.
 * Returns an empty array if the directory does not exist.
 */
export async function loadAgentDefinitions(dir: string): Promise<AgentDefinition[]> {
  let entries: string[]
  try {
    entries = await fs.readdir(dir)
  } catch {
    return []
  }

  const definitions: AgentDefinition[] = []
  for (const file of entries.filter((f) => f.endsWith('.md'))) {
    const content = await fs.readFile(path.join(dir, file), 'utf-8')
    definitions.push(parseAgentFile(content, file))
  }
  return definitions
}
