import { EventEmitter } from 'events'
import type { AgentEvent, EventFilter, EventSink, EventSource } from './types.js'

function matchesFilter(event: AgentEvent, filter: EventFilter): boolean {
  if (filter.agent_id && event.agent_id !== filter.agent_id) return false
  if (filter.session_id && event.session_id !== filter.session_id) return false
  if (filter.types && filter.types.length > 0 && !filter.types.includes(event.type)) return false
  return true
}

export class AgentEventEmitter implements EventSink, EventSource {
  private emitter = new EventEmitter()

  emit(event: AgentEvent): void {
    this.emitter.emit('event', event)
  }

  subscribe(filter: EventFilter): AsyncIterable<AgentEvent> {
    const emitter = this.emitter
    return {
      [Symbol.asyncIterator]() {
        const queue: AgentEvent[] = []
        let resolve: ((value: IteratorResult<AgentEvent>) => void) | null = null
        let done = false

        const listener = (event: AgentEvent) => {
          if (!matchesFilter(event, filter)) return
          if (resolve) {
            const r = resolve
            resolve = null
            r({ value: event, done: false })
          } else {
            queue.push(event)
          }
        }

        emitter.on('event', listener)

        return {
          next(): Promise<IteratorResult<AgentEvent>> {
            if (done) {
              return Promise.resolve({ value: undefined as unknown as AgentEvent, done: true })
            }
            if (queue.length > 0) {
              return Promise.resolve({ value: queue.shift()!, done: false })
            }
            return new Promise<IteratorResult<AgentEvent>>((r) => {
              resolve = r
            })
          },
          return(): Promise<IteratorResult<AgentEvent>> {
            done = true
            emitter.off('event', listener)
            if (resolve) {
              resolve({ value: undefined as unknown as AgentEvent, done: true })
              resolve = null
            }
            return Promise.resolve({ value: undefined as unknown as AgentEvent, done: true })
          },
          throw(err: unknown): Promise<IteratorResult<AgentEvent>> {
            done = true
            emitter.off('event', listener)
            if (resolve) {
              resolve({ value: undefined as unknown as AgentEvent, done: true })
              resolve = null
            }
            return Promise.reject(err)
          },
        }
      },
    }
  }

  subscribeOnce(filter: EventFilter): Promise<AgentEvent> {
    return new Promise<AgentEvent>((resolve) => {
      const listener = (event: AgentEvent) => {
        if (!matchesFilter(event, filter)) return
        this.emitter.off('event', listener)
        resolve(event)
      }
      this.emitter.on('event', listener)
    })
  }
}
