import { Hono } from 'hono'
import { streamSSE } from 'hono/streaming'
import { randomUUID } from 'node:crypto'
import { logger } from '../lib/logger.js'
import type { RuntimeContext } from '../lib/runtime.js'
import type { Item } from '../domain/types.js'
import { runAgent } from '../orchestrator/runner.js'
import { deliverResult, deliverApproval } from '../orchestrator/delivery.js'
import { EVENT_TYPES } from '../events/types.js'
import { cancelAgent } from '../domain/index.js'
import {
  buildDeps,
  formatAssistantOutput,
  prepareSessionTurn,
} from '../services/session-runner.js'

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

type ChatEnv = { Variables: { userId: string } }

export function chatRoutes(runtime: RuntimeContext) {
  const app = new Hono<ChatEnv>()

  // POST /completions — Start or continue a conversation
  app.post('/completions', async (c) => {
    try {
      const body = await c.req.json<{
        sessionId?: string
        model?: string
        agent?: string
        input: string | Item[]
        instructions?: string
        systemPrompt?: string
        tools?: string[]
        mcpServerIds?: string[]
        stream?: boolean
        temperature?: number
        maxTokens?: number
      }>()

      const userId = c.get('userId') as string
      const prepared = await prepareSessionTurn(runtime, {
        userId,
        sessionId: body.sessionId,
        model: body.model,
        agent: body.agent,
        input: body.input,
        instructions: body.instructions,
        systemPrompt: body.systemPrompt,
        mcpServerIds: body.mcpServerIds,
        allowedTools: body.tools,
        maxTokens: body.maxTokens,
      })

      if (prepared.status === 'active') {
        return c.json({
          id: prepared.agent.id,
          sessionId: prepared.sessionId,
          status: prepared.agent.status,
          output: prepared.output,
          waitingFor: prepared.agent.waitingFor.length > 0 ? prepared.agent.waitingFor : undefined,
        }, 202)
      }

      const { agent, sessionId, model } = prepared
      const deps = buildDeps(runtime, model)

      // 5. Streaming vs non-streaming
      if (body.stream) {
        return streamSSE(c, async (stream) => {
          const eventStream = runtime.events.subscribe({
            session_id: sessionId,
          })

          // Per-agent abort controller — cancel route aborts this to kill the run
          const agentAbort = new AbortController()
          runtime.agentAbortControllers.set(agent!.id, agentAbort)

          const signal = AbortSignal.any([
            c.req.raw.signal,
            runtime.shutdownController.signal,
            agentAbort.signal,
          ])

          const resultPromise = runAgent(agent!.id, deps, { stream: true, signal }).catch((err) => ({
            agentId: agent!.id,
            status: 'failed' as const,
            error: err instanceof Error ? err.message : String(err),
            turnCount: 0,
          }))

          const iterator = eventStream[Symbol.asyncIterator]()

          const mapEventToSSE = (event: { type: string; agent_id: string }): string | null => {
            if (
              (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
              event.agent_id === agent!.id
            ) return null

            switch (event.type) {
              case EVENT_TYPES.TEXT_DELTA: return 'text_delta'
              case EVENT_TYPES.COMPANION_TEXT: return 'text_delta'
              case EVENT_TYPES.TOOL_STARTED: return 'tool_start'
              case EVENT_TYPES.TOOL_COMPLETED: return 'tool_end'
              case EVENT_TYPES.TOOL_PROPOSED: return 'approval'
              case EVENT_TYPES.TOOL_APPROVED: return 'tool_approved'
              case EVENT_TYPES.TOOL_DENIED: return 'tool_denied'
              case EVENT_TYPES.AGENT_WAITING: return 'agent_status'
              case EVENT_TYPES.AGENT_STARTED: return 'agent_started'
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

          // Consume events until the agent finishes or waits
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
              event.agent_id === agent!.id
            ) break

            await writeEvent(event)
          }

          // Drain queued events — the iterator yields synchronously from its buffer
          // when events are already queued, so this loop finishes immediately once empty.
          while (true) {
            const pending = await Promise.race([
              iterator.next().then((v) => v),
              new Promise<null>((r) => setTimeout(() => r(null), 50)),
            ])
            if (!pending || pending.done) break
            const event = pending.value
            if (
              (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
              event.agent_id === agent!.id
            ) break
            await writeEvent(event)
          }

          // Send final result
          const result = await resultPromise
          runtime.agentAbortControllers.delete(agent!.id)

          await stream.writeSSE({
            event: 'done',
            data: JSON.stringify({
              id: agent!.id,
              sessionId,
              status: result.status,
              result: 'result' in result ? result.result : undefined,
              error: 'error' in result ? result.error : undefined,
              waitingFor: 'waitingFor' in result ? result.waitingFor : undefined,
              turnCount: result.turnCount,
            }),
            id: randomUUID(),
          })

          await iterator.return?.()
        })
      }

      // 6. Non-streaming: run agent synchronously
      const agentAbort = new AbortController()
      runtime.agentAbortControllers.set(agent.id, agentAbort)
      const result = await runAgent(agent.id, deps, {
        signal: AbortSignal.any([c.req.raw.signal, runtime.shutdownController.signal, agentAbort.signal]),
      })
      runtime.agentAbortControllers.delete(agent.id)

      // 7. Get output items
      const items = await runtime.repositories.items.listByAgent(agent.id)
      const output = formatAssistantOutput(items)

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
      const status = message.startsWith('Unknown agent:') || message.startsWith('Session not found:')
        ? 400
        : 500
      return c.json({ error: message }, status)
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

      // After runAndPropagateUp, result may be from a parent agent.
      // Return the root agent's items so the client sees the final response.
      const rootAgent = agent.parentId
        ? await runtime.repositories.agents.findRootAgent(agent.sessionId)
        : agent
      const itemsAgentId = rootAgent?.id ?? result.agentId ?? agentId
      const items = await runtime.repositories.items.listByAgent(itemsAgentId)
      const output = formatAssistantOutput(items)

      if (result.status === 'waiting') {
        return c.json({
          id: result.agentId ?? agentId,
          sessionId: agent.sessionId,
          status: 'waiting',
          output,
          waitingFor: result.waitingFor,
        }, 202)
      }

      return c.json({
        id: result.agentId ?? agentId,
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
        scope?: 'once' | 'conversation' | 'always'
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
        body.scope,
      )

      // After runAndPropagateUp, the result may be from a parent agent.
      // Always return the root agent's items so the client sees the final response.
      const rootAgent = agent.parentId
        ? await runtime.repositories.agents.findRootAgent(agent.sessionId)
        : agent
      const itemsAgentId = rootAgent?.id ?? result.agentId ?? agentId
      const items = await runtime.repositories.items.listByAgent(itemsAgentId)
      const output = formatAssistantOutput(items)

      if (result.status === 'waiting') {
        return c.json({
          id: result.agentId ?? agentId,
          sessionId: agent.sessionId,
          status: 'waiting',
          output,
          waitingFor: result.waitingFor,
        }, 202)
      }

      return c.json({
        id: result.agentId ?? agentId,
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

  // POST /agents/:agentId/cancel — Cancel a running or waiting agent
  app.post('/agents/:agentId/cancel', async (c) => {
    try {
      const { agentId } = c.req.param()
      const agent = await runtime.repositories.agents.getById(agentId)

      if (!agent) {
        return c.json({ error: `Agent not found: ${agentId}` }, 404)
      }

      if (agent.status === 'cancelled' || agent.status === 'completed') {
        return c.json({ id: agentId, status: agent.status })
      }

      // Abort the in-flight agent run so it stops immediately
      const abortController = runtime.agentAbortControllers.get(agentId)
      if (abortController) {
        abortController.abort()
        runtime.agentAbortControllers.delete(agentId)
      }

      const updated = cancelAgent(agent)
      await runtime.repositories.agents.update(agentId, {
        status: updated.status,
        completedAt: updated.completedAt,
      })

      return c.json({ id: agentId, status: 'cancelled' })
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

    // Break on root agent completion so that when a child agent is approved,
    // the event stream continues through the parent's resumed run.
    const rootAgent = agent.parentId
      ? await runtime.repositories.agents.findRootAgent(agent.sessionId)
      : agent
    const breakAgentId = rootAgent?.id ?? agentId

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

        // End the stream when the root agent is done (not just the requested agent)
        if (
          (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
          event.agent_id === breakAgentId
        ) {
          break
        }
      }
    })
  })

  return app
}
