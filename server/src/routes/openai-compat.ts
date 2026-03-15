/**
 * OpenAI-compatible /v1/chat/completions endpoint.
 *
 * This shim lets the TS server act as a drop-in "custom backend" in the
 * frontend UI.  The frontend's CustomProviderService sends standard
 * OpenAI-shaped requests and parses standard OpenAI-shaped responses —
 * this route translates between that format and the internal agent loop.
 *
 * Register in the frontend as a Custom Backend with:
 *   URL: http://localhost:3001/v1/chat/completions
 */

import { Hono } from 'hono'
import { stream } from 'hono/streaming'
import { randomUUID, createHash } from 'node:crypto'
import type { RuntimeContext } from '../lib/runtime.js'
import { runAgent } from '../orchestrator/runner.js'
import { EVENT_TYPES } from '../events/types.js'

// ---------------------------------------------------------------------------
// OpenAI-compatible types (subset we care about)
// ---------------------------------------------------------------------------

interface OAIMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

interface OAIRequest {
  model?: string
  messages: OAIMessage[]
  stream?: boolean
  temperature?: number
  max_tokens?: number
}

interface OAIChoice {
  index: number
  message: { role: 'assistant'; content: string }
  finish_reason: 'stop' | 'length'
}

interface OAIResponse {
  id: string
  object: 'chat.completion'
  created: number
  model: string
  choices: OAIChoice[]
  usage: { prompt_tokens: number; completion_tokens: number; total_tokens: number }
}

// ---------------------------------------------------------------------------
// Route
// ---------------------------------------------------------------------------

export function openaiCompatRoutes(runtime: RuntimeContext): Hono {
  const app = new Hono()

  app.post('/chat/completions', async (c) => {
    try {
      const body = await c.req.json<OAIRequest>()
      const { messages, stream: doStream } = body

      // Use the server's default model — the "model" field from the frontend
      // is the custom backend name, not a real model identifier.
      const model = runtime.config.defaultModel

      // --- Resolve dev user ---
      const devHash = createHash('sha256').update('dev-key').digest('hex')
      const devUser = await runtime.repositories.users.getByApiKeyHash(devHash)
      const userId = devUser?.id ?? 'unknown'

      // --- Extract system prompt and last user message from OpenAI messages ---
      const systemParts: string[] = []
      let lastUserContent = ''

      for (const msg of messages) {
        if (msg.role === 'system') {
          systemParts.push(msg.content)
        } else if (msg.role === 'user') {
          lastUserContent = msg.content
        }
      }

      if (!lastUserContent) {
        return c.json({ error: { message: 'No user message found', type: 'invalid_request_error' } }, 400)
      }

      // --- Create session + agent ---
      const session = await runtime.repositories.sessions.create({
        userId,
        title: lastUserContent.slice(0, 100),
      })

      const agent = await runtime.repositories.agents.create({
        sessionId: session.id,
        task: lastUserContent,
        config: {
          model,
          provider: model.indexOf(':') !== -1 ? model.slice(0, model.indexOf(':')) : model,
          max_turns: 50,
          max_tool_calls_per_step: 10,
          tool_execution_timeout_ms: 60_000,
        },
      })

      // --- Store conversation history as items ---
      // System prompt
      if (systemParts.length > 0) {
        await runtime.repositories.items.create({
          agentId: agent.id,
          type: 'message',
          role: 'system',
          content: systemParts.join('\n\n'),
          turnNumber: 0,
        })
      }

      // Prior conversation turns (all messages except the last user message,
      // which is already captured as the agent task)
      for (let i = 0; i < messages.length; i++) {
        const msg = messages[i]
        if (msg.role === 'system') continue // already stored
        // Skip the last user message — it's the task
        if (msg.role === 'user' && msg.content === lastUserContent && i === messages.length - 1) continue
        // Skip if this is a duplicate of the last user message
        if (msg.role === 'user' && i === messages.length - 1) continue

        await runtime.repositories.items.create({
          agentId: agent.id,
          type: 'message',
          role: msg.role,
          content: msg.content,
          turnNumber: 0,
        })
      }

      // Store the user message
      await runtime.repositories.items.create({
        agentId: agent.id,
        type: 'message',
        role: 'user',
        content: lastUserContent,
        turnNumber: 0,
      })

      // --- Resolve provider & build deps ---
      const deps = {
        agents: runtime.repositories.agents,
        items: runtime.repositories.items,
        toolOutputs: runtime.repositories.toolOutputs,
        provider: runtime.providers.resolve(model),
        tools: runtime.tools,
        events: runtime.events,
      }

      const completionId = `chatcmpl-${randomUUID()}`

      // =====================================================================
      // Streaming
      // =====================================================================
      if (doStream) {
        c.header('Content-Type', 'text/event-stream')
        c.header('Cache-Control', 'no-cache')
        c.header('Connection', 'keep-alive')

        return stream(c, async (s) => {
          const eventStream = runtime.events.subscribe({ session_id: session.id })
          const iterator = eventStream[Symbol.asyncIterator]()

          // Run agent in background
          const resultPromise = runAgent(agent.id, deps, { stream: true }).catch((err) => ({
            agentId: agent.id,
            status: 'failed' as const,
            error: err instanceof Error ? err.message : String(err),
            turnCount: 0,
          }))

          let done = false
          resultPromise.then(() => { done = true })

          // Helper to write a chunk in OpenAI SSE format
          const writeChunk = async (content: string, finishReason: string | null = null) => {
            const chunk = {
              id: completionId,
              object: 'chat.completion.chunk',
              created: Math.floor(Date.now() / 1000),
              model,
              choices: [{
                index: 0,
                delta: finishReason ? {} : { content },
                finish_reason: finishReason,
              }],
            }
            await s.write(`data: ${JSON.stringify(chunk)}\n\n`)
          }

          // Pipe events
          let sentDeltas = false
          while (!done) {
            const next = await Promise.race([
              iterator.next(),
              resultPromise.then(() => null),
            ])

            if (!next || next.done) break

            const event = next.value
            const payload = event.payload as Record<string, unknown> | undefined

            // Map agent events → OpenAI text_delta chunks
            if (event.type === EVENT_TYPES.TEXT_DELTA && payload?.text) {
              await writeChunk(String(payload.text))
              sentDeltas = true
            }

            if (event.type === EVENT_TYPES.COMPANION_TEXT && payload?.text) {
              await writeChunk(String(payload.text))
              sentDeltas = true
            }

            // Forward lifecycle events as raw JSON (not OpenAI format).
            // These have a `type` field but NO `choices` array, so the frontend
            // can distinguish them from text chunks.
            const forwardableEvents: string[] = [
              EVENT_TYPES.TOOL_STARTED,
              EVENT_TYPES.TOOL_COMPLETED,
              EVENT_TYPES.TOOL_PROPOSED,
              EVENT_TYPES.AGENT_STARTED,
              EVENT_TYPES.AGENT_COMPLETED,
              EVENT_TYPES.AGENT_FAILED,
              EVENT_TYPES.AGENT_WAITING,
              EVENT_TYPES.STEP_PROPOSED,
              EVENT_TYPES.STEP_STARTED,
              EVENT_TYPES.STEP_COMPLETED,
              EVENT_TYPES.TURN_STARTED,
              EVENT_TYPES.TURN_COMPLETED,
            ]

            if (forwardableEvents.includes(event.type)) {
              const eventData = {
                type: event.type,
                agentId: event.agent_id,
                sessionId: event.session_id,
                timestamp: event.timestamp,
                ...(payload ?? {}),
              }
              await s.write(`data: ${JSON.stringify(eventData)}\n\n`)
            }

            if (
              (event.type === EVENT_TYPES.AGENT_COMPLETED || event.type === EVENT_TYPES.AGENT_FAILED) &&
              event.agent_id === agent.id
            ) {
              break
            }
          }

          // Agent finished — get the final response text
          const result = await resultPromise
          const items = await runtime.repositories.items.listByAgent(agent.id)
          const assistantItems = items.filter((i) => i.type === 'message' && i.role === 'assistant')
          const finalContent = assistantItems.map((i) => i.content ?? '').join('\n').trim()

          // Send the final text only if no deltas were streamed
          if (!sentDeltas && finalContent) {
            await writeChunk(finalContent)
          }

          // Send finish
          await writeChunk('', 'stop')
          await s.write('data: [DONE]\n\n')

          await iterator.return?.()
        })
      }

      // =====================================================================
      // Non-streaming
      // =====================================================================
      const result = await runAgent(agent.id, deps)

      // Collect assistant output
      const items = await runtime.repositories.items.listByAgent(agent.id)
      const assistantItems = items.filter((i) => i.type === 'message' && i.role === 'assistant')
      const finalContent = assistantItems.map((i) => i.content ?? '').join('\n').trim()
        || result.result
        || (result.error ? `Error: ${result.error}` : '')

      const response: OAIResponse = {
        id: completionId,
        object: 'chat.completion',
        created: Math.floor(Date.now() / 1000),
        model,
        choices: [{
          index: 0,
          message: { role: 'assistant', content: finalContent },
          finish_reason: result.status === 'completed' ? 'stop' : 'length',
        }],
        usage: {
          prompt_tokens: 0,  // Not tracked at this level yet
          completion_tokens: 0,
          total_tokens: 0,
        },
      }

      return c.json(response)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return c.json({
        error: { message, type: 'server_error', code: 'internal_error' },
      }, 500)
    }
  })

  return app
}
