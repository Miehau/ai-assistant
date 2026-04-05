import fs from 'fs/promises'
import path from 'path'
import { logger } from '../lib/logger.js'
import type { WorkflowDefinition } from './types.js'

/**
 * Auto-discover workflow definitions from a directory.
 *
 * Convention: each `.js` file in the directory should export either:
 *   - `export const workflow: WorkflowDefinition` (named export)
 *   - `export default <WorkflowDefinition>`        (default export)
 *
 * In dev (tsx), `.ts` files are also loaded automatically.
 * Files starting with `_` or `.` are ignored (helpers, dotfiles).
 * Subdirectories are not scanned (flat structure).
 *
 * Workflows with `enabled: false` are logged and skipped.
 */
export async function loadWorkflowDefinitions(dir: string): Promise<WorkflowDefinition[]> {
  let entries: string[]
  try {
    entries = await fs.readdir(dir)
  } catch {
    // Directory doesn't exist — no workflows, that's fine
    return []
  }

  const files = entries.filter((f) => {
    if (f.startsWith('_') || f.startsWith('.')) return false
    return f.endsWith('.ts') || f.endsWith('.js')
  })

  const definitions: WorkflowDefinition[] = []

  for (const file of files) {
    const fullPath = path.join(dir, file)
    try {
      const mod = await import(fullPath)
      const def: WorkflowDefinition | undefined = mod.workflow ?? mod.default

      if (!def || typeof def.run !== 'function' || !def.name) {
        logger.warn({ file }, 'Skipping workflow file — no valid WorkflowDefinition export found')
        continue
      }

      if (def.enabled === false) {
        logger.info({ workflow: def.name, file }, 'Workflow disabled — skipping')
        continue
      }

      definitions.push(def)
      logger.debug({ workflow: def.name, file }, 'Workflow definition loaded')
    } catch (err) {
      logger.error({ err, file }, 'Failed to load workflow definition')
    }
  }

  return definitions
}
