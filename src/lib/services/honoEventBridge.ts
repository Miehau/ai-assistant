/**
 * Maps Hono server SSE events into the same AgentEvent format the chat store
 * already handles. This lets us reuse all existing downstream handlers
 * (streaming, tool activity, phase changes) without modification.
 */

import { getHttpBackend } from '$lib/backend/http-client';
import { AGENT_EVENT_TYPES } from '$lib/types/events';
import type { AgentEvent } from '$lib/types';

export interface HonoStreamOptions {
  conversationId: string;
  /** Locally-generated assistant message ID for store correlation */
  messageId: string;
  sessionId?: string;
  model?: string;
  instructions?: string;
  systemPrompt?: string;
}

export interface HonoStreamResult {
  /** The Hono session ID returned in the `done` event — persist for next turn */
  sessionId?: string;
}

/**
 * Streams a message through the Hono server and converts each SSE event into
 * an AgentEvent, passing it to `onEvent` so the existing chat store handlers
 * can process it unchanged.
 */
export async function streamMessageViaHono(
  input: string,
  options: HonoStreamOptions,
  onEvent: (event: AgentEvent) => void,
  signal: AbortSignal,
): Promise<HonoStreamResult> {
  const client = getHttpBackend();
  const ts = () => Date.now();
  const { conversationId, messageId } = options;

  onEvent({
    event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_STARTED,
    payload: { conversation_id: conversationId, message_id: messageId, timestamp_ms: ts() },
    timestamp_ms: ts(),
  });

  let fullText = '';
  let resultSessionId: string | undefined;
  const toolStartTimes = new Map<string, number>();
  // Track which approval callIds have already been surfaced to avoid duplicates
  const surfacedApprovalIds = new Set<string>();
  // True after a tool_end so the next text_delta gets a paragraph separator
  let pendingTurnSeparator = false;

  for await (const sseEvent of client.sendMessageStream(
    input,
    { sessionId: options.sessionId, model: options.model, instructions: options.instructions, systemPrompt: options.systemPrompt },
    signal,
  )) {
    const { event, data } = sseEvent;

    if (event === 'text_delta') {
      let chunk = (data.text as string) ?? '';
      if (pendingTurnSeparator && chunk.length > 0) {
        chunk = '\n\n' + chunk;
        pendingTurnSeparator = false;
      }
      fullText += chunk;
      onEvent({
        event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_CHUNK,
        payload: { conversation_id: conversationId, message_id: messageId, chunk, timestamp_ms: ts() },
        timestamp_ms: ts(),
      });
    } else if (event === 'tool_start') {
      const callId = data.callId as string;
      const parentId = data.parentId as string | null | undefined;
      toolStartTimes.set(callId, Date.now());
      onEvent({
        event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED,
        payload: {
          execution_id: callId,
          tool_name: data.name as string,
          args: (data.args as Record<string, unknown>) ?? {},
          message_id: messageId,
          conversation_id: conversationId,
          session_id: data.agentId as string | undefined,
          parent_session_id: parentId ?? null,
          is_sub_agent: parentId != null,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'tool_end') {
      const callId = data.callId as string;
      const started = toolStartTimes.get(callId) ?? Date.now();
      const success = data.success !== false;
      const parentId = data.parentId as string | null | undefined;
      pendingTurnSeparator = fullText.length > 0;
      onEvent({
        event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_COMPLETED,
        payload: {
          execution_id: callId,
          tool_name: (data.name as string) ?? '',
          result: data.output,
          success,
          error: success ? undefined : String(data.output ?? 'Tool failed'),
          duration_ms: Date.now() - started,
          message_id: messageId,
          conversation_id: conversationId,
          session_id: data.agentId as string | undefined,
          parent_session_id: parentId ?? null,
          is_sub_agent: parentId != null,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'approval') {
      const callId = data.callId as string;
      const toolName = data.name as string;
      console.log('[honoEventBridge] approval required (approval SSE path):', toolName, callId);
      surfacedApprovalIds.add(callId);
      onEvent({
        event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_PROPOSED,
        payload: {
          execution_id: callId,
          approval_id: callId,
          tool_name: toolName,
          args: (data.args as Record<string, unknown>) ?? {},
          agent_id: data.agentId as string | undefined,
          message_id: messageId,
          conversation_id: conversationId,
          iteration: 0,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'tool_approved') {
      const callId = data.callId as string;
      onEvent({
        event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_APPROVED,
        payload: {
          execution_id: callId,
          approval_id: callId,
          tool_name: (data.name as string) ?? '',
          message_id: messageId,
          conversation_id: conversationId,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'tool_denied') {
      const callId = data.callId as string;
      onEvent({
        event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_DENIED,
        payload: {
          execution_id: callId,
          approval_id: callId,
          tool_name: (data.name as string) ?? '',
          message_id: messageId,
          conversation_id: conversationId,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'agent_status') {
      // AGENT_WAITING payload has `waitingFor` (not `status`) — handle both paths.
      const waitingFor = data.waitingFor as Array<{
        callId: string; type: string; name: string; args?: Record<string, unknown>;
      }> | undefined;

      if (waitingFor && waitingFor.length > 0) {
        // Emit phase change so the UI knows the agent is paused
        onEvent({
          event_type: AGENT_EVENT_TYPES.AGENT_PHASE_CHANGED,
          payload: { phase: 'WaitingForHumanInput', timestamp_ms: ts() },
          timestamp_ms: ts(),
        });
        // Surface each pending approval — skip those already surfaced via the `approval` SSE event
        for (const entry of waitingFor) {
          if (entry.type === 'approval' && !surfacedApprovalIds.has(entry.callId)) {
            console.log('[honoEventBridge] approval required (agent_status path):', entry.name, entry.callId);
            surfacedApprovalIds.add(entry.callId);
            onEvent({
              event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_PROPOSED,
              payload: {
                execution_id: entry.callId,
                approval_id: entry.callId,
                tool_name: entry.name,
                args: entry.args ?? {},
                agent_id: data.agentId as string | undefined,
                message_id: messageId,
                conversation_id: conversationId,
                iteration: 0,
                timestamp_ms: ts(),
              },
              timestamp_ms: ts(),
            });
          }
        }
      } else {
        // Fallback: try the old status-based mapping
        const phase = honoStatusToPhase(data.status as string);
        if (phase) {
          onEvent({
            event_type: AGENT_EVENT_TYPES.AGENT_PHASE_CHANGED,
            payload: { phase, timestamp_ms: ts() },
            timestamp_ms: ts(),
          });
        }
      }
    } else if (event === 'done') {
      resultSessionId = (data.sessionId as string | undefined)
        ?? ((data.response as Record<string, unknown> | undefined)?.sessionId as string | undefined);

      // Propagate agent failures as thrown errors so the chat store shows an error state
      // rather than silently completing with empty content (which gets filtered out).
      if (data.status === 'failed') {
        const errorMsg = (data.error as string) ?? 'Agent run failed';
        throw new Error(errorMsg);
      }

      // Final-fallback: if agent ended waiting with approvals not yet surfaced, surface them now
      if (data.status === 'waiting') {
        const waitingFor = data.waitingFor as Array<{
          callId: string; type: string; name: string; args?: Record<string, unknown>;
        }> | undefined;
        if (waitingFor) {
          for (const entry of waitingFor) {
            if (entry.type === 'approval' && !surfacedApprovalIds.has(entry.callId)) {
              console.log('[honoEventBridge] approval required (done path):', entry.name, entry.callId);
              surfacedApprovalIds.add(entry.callId);
              onEvent({
                event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_PROPOSED,
                payload: {
                  execution_id: entry.callId,
                  approval_id: entry.callId,
                  tool_name: entry.name,
                  args: entry.args ?? {},
                  agent_id: data.id as string | undefined,
                  message_id: messageId,
                  conversation_id: conversationId,
                  iteration: 0,
                  timestamp_ms: ts(),
                },
                timestamp_ms: ts(),
              });
            }
          }
        }
      }

      // Prefer the server's parsed result over raw streamed text when non-empty.
      // For native providers (anthropic, openai, openrouter) the model streams
      // text via text_delta and `data.result` holds the same final text — either
      // works as the authoritative content.  Use `||` (not `??`) so an empty
      // string `result` falls back to accumulated fullText from text_delta events.
      const serverResult = data.result as string | undefined;
      const content = serverResult || fullText;

      onEvent({
        event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_COMPLETED,
        payload: {
          conversation_id: conversationId,
          message_id: messageId,
          content,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'error') {
      const msg = (data.error as string) ?? (data.message as string) ?? 'Hono stream error';
      throw new Error(msg);
    }
  }

  return { sessionId: resultSessionId };
}

function honoStatusToPhase(status: string): string | null {
  switch (status) {
    case 'running':   return 'Executing';
    case 'waiting':   return 'WaitingForHumanInput';
    case 'completed': return 'Completed';
    case 'failed':    return 'Failed';
    default:          return null;
  }
}
