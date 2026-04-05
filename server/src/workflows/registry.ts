import type { WorkflowDefinition, WorkflowRegistry } from './types.js'

export class WorkflowRegistryImpl implements WorkflowRegistry {
  private definitions = new Map<string, WorkflowDefinition>()

  register<I, O>(def: WorkflowDefinition<I, O>): void {
    if (this.definitions.has(def.name)) {
      throw new Error(`Workflow already registered: ${def.name}`)
    }
    this.definitions.set(def.name, def as WorkflowDefinition)
  }

  get(name: string): WorkflowDefinition | undefined {
    return this.definitions.get(name)
  }

  list(): WorkflowDefinition[] {
    return [...this.definitions.values()]
  }
}
