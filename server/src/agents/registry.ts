import type { AgentDefinition } from './types.js'
import { loadAgentDefinitions } from './loader.js'

export interface AgentDefinitionRegistry {
  get(name: string): AgentDefinition | undefined
  list(): AgentDefinition[]
  /** Re-read agent definitions from disk. */
  reload(): Promise<void>
}

export class AgentDefinitionRegistryImpl implements AgentDefinitionRegistry {
  private definitions: Map<string, AgentDefinition>
  private readonly dir: string

  constructor(dir: string, definitions: AgentDefinition[] = []) {
    this.dir = dir
    this.definitions = new Map(definitions.map((d) => [d.name, d]))
  }

  get(name: string): AgentDefinition | undefined {
    return this.definitions.get(name)
  }

  list(): AgentDefinition[] {
    return [...this.definitions.values()]
  }

  async reload(): Promise<void> {
    const defs = await loadAgentDefinitions(this.dir)
    this.definitions = new Map(defs.map((d) => [d.name, d]))
  }
}
