import { Hono } from 'hono'
import { streamSSE } from 'hono/streaming'
import { randomUUID } from 'node:crypto'
import { logger } from '../lib/logger.js'
import type { RuntimeContext } from '../lib/runtime.js'
import type { OrchestratorDeps } from '../orchestrator/types.js'
import type { Item } from '../domain/types.js'
import { runAgent } from '../orchestrator/runner.js'
import { deliverResult, deliverApproval } from '../orchestrator/delivery.js'
import { EVENT_TYPES } from '../events/types.js'

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function resolveProvider(runtime: RuntimeContext, model: string) {
  return runtime.providers.resolve(model)
}

function extractProviderName(model: string): string {
  const idx = model.indexOf(':')
  return idx === -1 ? model : model.slice(0, idx)
}

function buildDeps(runtime: RuntimeContext, model: string): OrchestratorDeps {
  return {
    agents: runtime.repositories.agents,
    items: runtime.repositories.items,
    toolOutputs: runtime.repositories.toolOutputs,
    provider: resolveProvider(runtime, model),
    tools: runtime.tools,
    events: runtime.events,
  }
}

function formatOutput(items: Item[]): Item[] {
  return items.filter(
    (i) => i.type === 'message' && i.role === 'assistant',
  )
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

export function chatRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  // POST /completions — Start or continue a conversation
  app.post('/completions', async (c) => {
    try {
      const body = await c.req.json<{
        sessionId?: string
        model?: string
        input: string | Item[]
        instructions?: string
        tools?: string[]
        stream?: boolean
        temperature?: number
        maxTokens?: number
      }>()

      const model = body.model ?? runtime.config.defaultModel

      // Resolve user — look up dev user by API key hash for now
      // TODO: extract from auth middleware when auth is wired
      const { createHash } = await import('node:crypto')
      const devHash = createHash('sha256').update('dev-key').digest('hex')
      const devUser = await runtime.repositories.users.getByApiKeyHash(devHash)
      const userId = devUser?.id ?? 'unknown'

      // 1. Get or create session
      let sessionId = body.sessionId
      if (!sessionId) {
        const session = await runtime.repositories.sessions.create({
          userId,
          title: typeof body.input === 'string'
            ? body.input.slice(0, 100)
            : 'New conversation',
        })
        sessionId = session.id
      } else {
        const existing = await runtime.repositories.sessions.getById(sessionId)
        if (!existing) {
          return c.json({ error: `Session not found: ${sessionId}` }, 404)
        }
      }

      // 2. Create agent with task from input
      const task = typeof body.input === 'string'
        ? body.input
        : JSON.stringify(body.input)

      const agent = await runtime.repositories.agents.create({
        sessionId,
        task,
        config: {
          model,
          provider: extractProviderName(model),
          max_turns: 50,
          max_tool_calls_per_step: 10,
          tool_execution_timeout_ms: 60_000,
        },
      })

      // Store user input as an item
      if (typeof body.input === 'string') {
        await runtime.repositories.items.create({
          agentId: agent.id,
          type: 'message',
          role: 'user',
          content: body.input,
          turnNumber: 0,
        })
      } else if (Array.isArray(body.input)) {
        for (const item of body.input) {
          await runtime.repositories.items.create({
            agentId: agent.id,
            type: item.type ?? 'message',
            role: item.role ?? 'user',
            content: item.content ?? null,
            callId: item.callId ?? null,
            name: item.name ?? null,
            arguments: item.arguments ?? null,
            output: item.output ?? null,
            isError: item.isError ?? null,
            turnNumber: 0,
          })
        }
      }

      // Store instructions as system message if provided
      if (body.instructions) {
        await runtime.repositories.items.create({
          agentId: agent.id,
          type: 'message',
          role: 'system',
          content: body.instructions,
          turnNumber: 0,
        })
      }

      // 3. Resolve provider and build deps
      const deps = buildDeps(runtime, model)

      // 4. Streaming vs non-streaming
      if (body.stream) {
        return streamSSE(c, async (stream) => {
          // Subscribe to events for this agent
          const eventStream = runtime.events.subscribe({
            session_id: sessionId,
          })

          // Awaited text delta writer — each chunk is flushed before the next arrives
          const writeTextDelta = async (text: string) => {
            await stream.writeSSE({
              event: 'text_delta',
              data: JSON.stringify({ type: 'text_delta', agentId: agent.id, sessionId, text }),
              id: randomUUID(),
            })
          }

          // Run the agent in background with direct text delta callback
          const resultPromise = runAgent(agent.id, deps, { stream: true, onTextDelta: writeTextDelta }).catch((err) => ({
            agentId: agent.id,
            status: 'failed' as const,
            error: err instanceof Error ? err.message : String(err),
            turnCount: 0,
          }))

          // Pipe non-text events as SSE (text_delta already handled by onTextDelta)
          const iterator = eventStream[Symbol.asyncIterator]()

          // Maps an internal event to an SSE event name, or null to skip
          const mapEventToSSE = (event: { type: string; agent_id: string }): string | null => {
            if (event.type === EVENT_TYPES.TEXT_DELTA) return null
            // Root agent done/failed handled separately
            if (
              (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
              event.agent_id === agent.id
            ) return null

            switch (event.type) {
              case EVENT_TYPES.COMPANION_TEXT: return 'text_delta'
              case EVENT_TYPES.TOOL_STARTED: return 'tool_start'
              case EVENT_TYPES.TOOL_COMPLETED: return 'tool_end'
              case EVENT_TYPES.TOOL_PROPOSED: return 'approval'
              case EVENT_TYPES.TOOL_APPROVED: return 'tool_approved'
              case EVENT_TYPES.TOOL_DENIED: return 'tool_denied'
              case EVENT_TYPES.AGENT_WAITING: return 'agent_status'
              case EVENT_TYPES.AGENT_COMPLETED: return 'subagent_done'
              case EVENT_TYPES.AGENT_FAILED: return 'subagent_error'
              default: return event.type
            }
          }

          const writeEvent = async (event: { type: string; agent_id: string; payload: unknown }) => {
            const sseEvent = mapEventToSSE(event)
            if (!sseEvent) return
            await stream.writeSSE({
              event: sseEvent,
              data: JSON.stringify({
                type: sseEvent,
                agentId: event.agent_id,
                sessionId,
                ...event.payload as Record<string, unknown>,
              }),
              id: randomUUID(),
            })
          }

          // Phase 1: consume events until the agent finishes or waits
          let agentDone = false
          resultPromise.then(() => { agentDone = true })

          while (!agentDone) {
            const next = await Promise.race([
              iterator.next(),
              resultPromise.then(() => null),
            ])

            if (!next || next.done) break

            const event = next.value
            if (
              (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
              event.agent_id === agent.id
            ) break

            await writeEvent(event)
          }

          // Phase 2: drain any events that were queued before the agent finished
          // but not yet consumed (race condition between event emission and loop exit).
          // A short timeout detects when the queue is empty.
          while (true) {
            const pending = await Promise.race([
              iterator.next().then((v) => v),
              new Promise<null>((r) => setTimeout(() => r(null), 50)),
            ])
            if (!pending || pending.done) break
            const event = pending.value
            if (
              (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
              event.agent_id === agent.id
            ) break
            await writeEvent(event)
          }

          // Send final result
          const result = await resultPromise
          await stream.writeSSE({
            event: 'done',
            data: JSON.stringify({
              id: agent.id,
              sessionId,
              status: result.status,
              result: 'result' in result ? result.result : undefined,
              error: 'error' in result ? result.error : undefined,
              waitingFor: 'waitingFor' in result ? result.waitingFor : undefined,
              turnCount: result.turnCount,
            }),
            id: randomUUID(),
          })

          // Clean up the iterator
          await iterator.return?.()
        })
      }

      // 5. Non-streaming: run agent synchronously
      const result = await runAgent(agent.id, deps)

      // 6. Get output items
      const items = await runtime.repositories.items.listByAgent(agent.id)
      const output = formatOutput(items)

      if (result.status === 'waiting') {
        return c.json({
          id: agent.id,
          sessionId,
          status: 'waiting',
          output,
          waitingFor: result.waitingFor,
        }, 202)
      }

      return c.json({
        id: agent.id,
        sessionId,
        status: result.status,
        output,
        usage: { turnCount: result.turnCount },
        error: result.error,
      }, 200)
    } catch (err) {
      logger.error(err, 'POST /completions failed')
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // POST /agents/:agentId/deliver — Deliver tool result to waiting agent
  app.post('/agents/:agentId/deliver', async (c) => {
    try {
      const { agentId } = c.req.param()
      const body = await c.req.json<{
        callId: string
        output: string
        isError?: boolean
      }>()

      const agent = await runtime.repositories.agents.getById(agentId)
      if (!agent) {
        return c.json({ error: `Agent not found: ${agentId}` }, 404)
      }

      const deps = buildDeps(runtime, agent.config.model)
      const result = await deliverResult(
        agentId,
        body.callId,
        body.output,
        body.isError ?? false,
        deps,
      )

      const items = await runtime.repositories.items.listByAgent(agentId)
      const output = formatOutput(items)

      if (result.status === 'waiting') {
        return c.json({
          id: agentId,
          sessionId: agent.sessionId,
          status: 'waiting',
          output,
          waitingFor: result.waitingFor,
        }, 202)
      }

      return c.json({
        id: agentId,
        sessionId: agent.sessionId,
        status: result.status,
        output,
        usage: { turnCount: result.turnCount },
        error: result.error,
      }, 200)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // POST /agents/:agentId/approve — Approve/deny pending tool
  app.post('/agents/:agentId/approve', async (c) => {
    try {
      const { agentId } = c.req.param()
      const body = await c.req.json<{
        callId: string
        decision: 'approved' | 'denied'
      }>()

      const agent = await runtime.repositories.agents.getById(agentId)
      if (!agent) {
        return c.json({ error: `Agent not found: ${agentId}` }, 404)
      }

      const deps = buildDeps(runtime, agent.config.model)
      const result = await deliverApproval(
        agentId,
        body.callId,
        body.decision,
        deps,
      )

      const items = await runtime.repositories.items.listByAgent(agentId)
      const output = formatOutput(items)

      if (result.status === 'waiting') {
        return c.json({
          id: agentId,
          sessionId: agent.sessionId,
          status: 'waiting',
          output,
          waitingFor: result.waitingFor,
        }, 202)
      }

      return c.json({
        id: agentId,
        sessionId: agent.sessionId,
        status: result.status,
        output,
        usage: { turnCount: result.turnCount },
        error: result.error,
      }, 200)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // GET /agents/:agentId — Get agent status
  app.get('/agents/:agentId', async (c) => {
    try {
      const { agentId } = c.req.param()
      const agent = await runtime.repositories.agents.getById(agentId)

      if (!agent) {
        return c.json({ error: `Agent not found: ${agentId}` }, 404)
      }

      return c.json({
        id: agent.id,
        sessionId: agent.sessionId,
        status: agent.status,
        waitingFor: agent.waitingFor.length > 0 ? agent.waitingFor : undefined,
        result: agent.result,
        error: agent.error,
        turnCount: agent.turnCount,
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({ error: message }, 500)
    }
  })

  // GET /agents/:agentId/events — SSE stream for agent events
  app.get('/agents/:agentId/events', async (c) => {
    const { agentId } = c.req.param()

    const agent = await runtime.repositories.agents.getById(agentId)
    if (!agent) {
      return c.json({ error: `Agent not found: ${agentId}` }, 404)
    }

    return streamSSE(c, async (stream) => {
      const eventStream = runtime.events.subscribe({
        session_id: agent.sessionId,
      })

      for await (const event of eventStream) {
        await stream.writeSSE({
          event: event.type,
          data: JSON.stringify({
            type: event.type,
            agentId: event.agent_id,
            sessionId: agent.sessionId,
            ...event.payload as Record<string, unknown>,
          }),
          id: randomUUID(),
        })

        // End the stream only when the root agent (the one requested) is done
        if (
          (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
          event.agent_id === agentId
        ) {
          break
        }
      }
    })
  })

  return app
}
