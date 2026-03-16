import { EventEmitter } from 'events'
import type { AgentEvent, EventFilter, EventSink, EventSource } from './types.js'

// Drop oldest events when a subscriber falls behind — prevents unbounded memory growth
// under slow SSE clients or paused consumers.
const MAX_QUEUE_SIZE = 500

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
            if (queue.length >= MAX_QUEUE_SIZE) {
              queue.shift() // drop oldest to maintain backpressure bound
            }
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

  subscribeOnce(filter: EventFilter, signal?: AbortSignal): Promise<AgentEvent> {
    return new Promise<AgentEvent>((resolve, reject) => {
      const listener = (event: AgentEvent) => {
        if (!matchesFilter(event, filter)) return
        signal?.removeEventListener('abort', onAbort)
        this.emitter.off('event', listener)
        resolve(event)
      }

      const onAbort = () => {
        this.emitter.off('event', listener)
        reject(new DOMException('subscribeOnce aborted', 'AbortError'))
      }

      this.emitter.on('event', listener)
      signal?.addEventListener('abort', onAbort, { once: true })
    })
  }
}
