/**
 * Maps Hono server SSE events into the same AgentEvent format the chat store
 * already handles. This lets us reuse all existing downstream handlers
 * (streaming, tool activity, phase changes) without modification.
 *
 * Sub-agent isolation: text deltas from child agents are accumulated separately
 * and do NOT bleed into the parent message stream. Sub-agent lifecycle events
 * (started/completed/failed) are emitted so the store can track them.
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
  /** Named agent definition to use (e.g. "planner"). Applies its model, system prompt, and tool restrictions. */
  agent?: string;
  instructions?: string;
  systemPrompt?: string;
  /** Called as soon as the server's session ID is known (first SSE event).
   *  Persist it immediately so follow-up messages don't lose context on abort. */
  onSessionId?: (sessionId: string) => void;
  /** Called as soon as the root agent ID is known — persist eagerly so cancel
   *  works even if the stream is aborted before the `done` event. */
  onAgentId?: (agentId: string) => void;
}

export interface HonoStreamResult {
  /** The Hono session ID returned in the `done` event — persist for next turn */
  sessionId?: string;
  /** Root agent ID — used to cancel the agent if needed */
  agentId?: string;
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

  // --- Per-agent text isolation ---
  // Only root-agent text feeds into the parent message stream.
  // Sub-agent text is accumulated separately and surfaces via subagent_done.
  let rootAgentId: string | undefined;
  let rootText = '';
  let resultSessionId: string | undefined;
  const toolStartTimes = new Map<string, number>();
  const surfacedApprovalIds = new Set<string>();
  // Per-agent turn separator: after a tool_end for a given agent, the next
  // text_delta from that agent gets a paragraph break.
  const pendingTurnSeparator = new Map<string, boolean>();

  // --- Delegate ↔ child agent text routing ---
  // Maps the agentId that CALLED delegate → the delegate tool's callId.
  // When a child's text_delta arrives, parentId matches the caller's agentId.
  const delegateCallByParent = new Map<string, { callId: string; depth: number }>();
  // Accumulated child-agent text per delegate callId.
  const delegateChildText = new Map<string, string>();

  /** Check if an agentId belongs to a sub-agent (not the root). */
  function isSubAgent(agentId: string | undefined, parentId: string | null | undefined): boolean {
    if (parentId != null) return true;
    if (rootAgentId && agentId && agentId !== rootAgentId) return true;
    return false;
  }

  for await (const sseEvent of client.sendMessageStream(
    input,
    { sessionId: options.sessionId, model: options.model, agent: options.agent, instructions: options.instructions, systemPrompt: options.systemPrompt },
    signal,
  )) {
    const { event, data } = sseEvent;

    // Capture sessionId from the first event that carries it — the server includes
    // sessionId in every SSE event, so we don't lose it if the stream is aborted
    // before the `done` event arrives.
    if (!resultSessionId && data.sessionId) {
      resultSessionId = data.sessionId as string;
      options.onSessionId?.(resultSessionId);
    }

    if (event !== 'text_delta') {
      console.log('[honoEventBridge] SSE event:', event, data.name ?? data.status ?? '', data.callId ?? '');
    }

    if (event === 'text_delta') {
      const agentId = data.agentId as string | undefined;
      const parentId = data.parentId as string | null | undefined;

      // Track root agent from first text event
      if (!rootAgentId && !parentId && agentId) {
        rootAgentId = agentId;
        options.onAgentId?.(agentId);
      }

      // Sub-agent text: do NOT emit as ASSISTANT_STREAM_CHUNK (would corrupt parent message).
      // Instead, route it into the delegate tool call's output as a live preview.
      if (isSubAgent(agentId, parentId)) {
        const delegateInfo = parentId ? delegateCallByParent.get(parentId as string) : undefined;
        if (delegateInfo) {
          const text = (data.text as string) ?? '';
          const accumulated = (delegateChildText.get(delegateInfo.callId) ?? '') + text;
          delegateChildText.set(delegateInfo.callId, accumulated);
          onEvent({
            event_type: AGENT_EVENT_TYPES.AGENT_OUTPUT_DELTA,
            payload: {
              parent_execution_id: delegateInfo.callId,
              text: accumulated,
              depth: delegateInfo.depth,
              message_id: messageId,
              conversation_id: conversationId,
              timestamp_ms: ts(),
            },
            timestamp_ms: ts(),
          });
        }
        continue;
      }

      let chunk = (data.text as string) ?? '';
      const agentKey = agentId ?? '__root__';
      if (pendingTurnSeparator.get(agentKey) && chunk.length > 0) {
        chunk = '\n\n' + chunk;
        pendingTurnSeparator.set(agentKey, false);
      }
      rootText += chunk;
      onEvent({
        event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_CHUNK,
        payload: { conversation_id: conversationId, message_id: messageId, chunk, timestamp_ms: ts() },
        timestamp_ms: ts(),
      });
    } else if (event === 'tool_start') {
      const callId = data.callId as string;
      const parentId = data.parentId as string | null | undefined;
      toolStartTimes.set(callId, Date.now());

      // Track delegate tool calls so we can route child text into them
      if ((data.name as string) === 'delegate') {
        const callerAgentId = data.agentId as string;
        const depth = ((data.depth as number) ?? 0) + 1;
        delegateCallByParent.set(callerAgentId, { callId, depth });
        delegateChildText.set(callId, '');
      }

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
      const agentId = data.agentId as string | undefined;
      console.log('[honoEventBridge] tool_end:', data.name, callId, { success, agentId, parentId });

      // Clean up delegate tracking when the delegate tool completes
      if ((data.name as string) === 'delegate' && agentId) {
        delegateCallByParent.delete(agentId);
        delegateChildText.delete(callId);
      }

      // Only set turn separator for the agent that owns this tool call
      const agentKey = agentId ?? '__root__';
      if (!isSubAgent(agentId, parentId) && rootText.length > 0) {
        pendingTurnSeparator.set(agentKey, true);
      }

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
          session_id: agentId,
          parent_session_id: parentId ?? null,
          is_sub_agent: parentId != null,
          timestamp_ms: ts(),
        },
        timestamp_ms: ts(),
      });
    } else if (event === 'agent_started') {
      // Track root agent ID for text isolation; sub-agent activity is already
      // tracked via tool_start/tool_end events with parentId/is_sub_agent.
      const agentId = data.agentId as string;
      const parentId = data.parentId as string | null | undefined;
      if (!rootAgentId && !parentId) {
        rootAgentId = agentId;
        options.onAgentId?.(agentId);
      }
    } else if (event === 'subagent_done' || event === 'subagent_error') {
      // Sub-agent lifecycle — no separate client event needed.
      // The delegate tool_start/tool_end already surfaces in toolActivity.
    } else if (event === 'approval') {
      const callId = data.callId as string;
      const toolName = data.name as string;
      const parentId = data.parentId as string | null | undefined;
      console.log('[honoEventBridge] approval required (approval SSE path):', toolName, callId);
      surfacedApprovalIds.add(callId);

      // Create a tool call record so the tool bubble appears in the message
      // (approval-required tools don't emit tool_start before the approval event)
      toolStartTimes.set(callId, Date.now());
      onEvent({
        event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED,
        payload: {
          execution_id: callId,
          tool_name: toolName,
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
          payload: { phase: 'WaitingForHumanInput', timestamp_ms: ts() } as any,
          timestamp_ms: ts(),
        });
        // Surface each pending approval — skip those already surfaced via the `approval` SSE event
        for (const entry of waitingFor) {
          if (entry.type === 'approval' && !surfacedApprovalIds.has(entry.callId)) {
            console.log('[honoEventBridge] approval required (agent_status path):', entry.name, entry.callId);
            surfacedApprovalIds.add(entry.callId);
            // Emit STARTED so a tool bubble is created (segment + ToolCallRecord)
            toolStartTimes.set(entry.callId, Date.now());
            onEvent({
              event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED,
              payload: {
                execution_id: entry.callId,
                tool_name: entry.name,
                args: entry.args ?? {},
                message_id: messageId,
                conversation_id: conversationId,
                session_id: data.agentId as string | undefined,
                parent_session_id: null,
                is_sub_agent: false,
                timestamp_ms: ts(),
              },
              timestamp_ms: ts(),
            });
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
            payload: { phase, timestamp_ms: ts() } as any,
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
              // Emit STARTED so a tool bubble is created (segment + ToolCallRecord)
              toolStartTimes.set(entry.callId, Date.now());
              onEvent({
                event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED,
                payload: {
                  execution_id: entry.callId,
                  tool_name: entry.name,
                  args: entry.args ?? {},
                  message_id: messageId,
                  conversation_id: conversationId,
                  session_id: data.id as string | undefined,
                  parent_session_id: null,
                  is_sub_agent: false,
                  timestamp_ms: ts(),
                },
                timestamp_ms: ts(),
              });
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

      // Use the accumulated rootText as the authoritative content — it matches
      // exactly what was streamed via ASSISTANT_STREAM_CHUNK events (including
      // turn separators). Only fall back to the server's result when rootText
      // is empty (non-streaming path or no text output).
      const serverResult = data.result as string | undefined;
      const content = rootText || serverResult || '';

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

  return { sessionId: resultSessionId, agentId: rootAgentId };
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
