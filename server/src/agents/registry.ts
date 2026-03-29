import type { AgentDefinition } from './types.js'

export interface AgentDefinitionRegistry {
  get(name: string): AgentDefinition | undefined
  list(): AgentDefinition[]
}

export class AgentDefinitionRegistryImpl implements AgentDefinitionRegistry {
  private readonly definitions: Map<string, AgentDefinition>

  constructor(definitions: AgentDefinition[] = []) {
    this.definitions = new Map(definitions.map((d) => [d.name, d]))
  }

  get(name: string): AgentDefinition | undefined {
    return this.definitions.get(name)
  }

  list(): AgentDefinition[] {
    return [...this.definitions.values()]
  }
}
