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

  for await (const sseEvent of client.sendMessageStream(
    input,
    { sessionId: options.sessionId, model: options.model, instructions: options.instructions },
    signal,
  )) {
    const { event, data } = sseEvent;

    if (event === 'text_delta') {
      const chunk = (data.text as string) ?? '';
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
      onEvent({
        event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_PROPOSED,
        payload: {
          approval_id: data.callId as string,
          tool_name: data.name as string,
          args: (data.args as Record<string, unknown>) ?? {},
          message_id: messageId,
          conversation_id: conversationId,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'agent_status') {
      const phase = honoStatusToPhase(data.status as string);
      if (phase) {
        onEvent({
          event_type: AGENT_EVENT_TYPES.AGENT_PHASE_CHANGED,
          payload: { phase, timestamp_ms: ts() },
          timestamp_ms: ts(),
        });
      }
    } else if (event === 'done') {
      resultSessionId = (data.sessionId as string | undefined)
        ?? ((data.response as Record<string, unknown> | undefined)?.sessionId as string | undefined);
      onEvent({
        event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_COMPLETED,
        payload: {
          conversation_id: conversationId,
          message_id: messageId,
          content: fullText,
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
