/**
 * Per-agent mutex. Serializes concurrent deliver/approve operations
 * to prevent read-modify-write races on agent state.
 */
export class AgentLock {
  private locks = new Map<string, Promise<void>>()

  async acquire(agentId: string): Promise<() => void> {
    // Wait for any existing lock on this agent to release
    while (this.locks.has(agentId)) {
      await this.locks.get(agentId)
    }

    let release!: () => void
    const promise = new Promise<void>((r) => {
      release = r
    })
    this.locks.set(agentId, promise)

    return () => {
      this.locks.delete(agentId)
      release()
    }
  }
}
