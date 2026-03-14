import type { ToolCallRecord } from '$lib/types';

export interface ToolCallGroup {
  sessionId: string | undefined;
  isSubAgent: boolean;
  parentSessionId: string | null | undefined;
  calls: ToolCallRecord[];
}

/**
 * Groups tool calls by session ID to enable hierarchical display.
 * Main agent tool calls are returned as individual groups.
 * Subagent tool calls are grouped by their session_id.
 */
export function groupToolCallsBySession(toolCalls: ToolCallRecord[]): ToolCallGroup[] {
  const groups: ToolCallGroup[] = [];
  const sessionMap = new Map<string, ToolCallRecord[]>();

  for (const call of toolCalls) {
    if (call.is_sub_agent && call.session_id) {
      // Group subagent calls by session
      const existing = sessionMap.get(call.session_id);
      if (existing) {
        existing.push(call);
      } else {
        sessionMap.set(call.session_id, [call]);
      }
    } else {
      // Main agent calls remain individual
      groups.push({
        sessionId: call.session_id,
        isSubAgent: false,
        parentSessionId: call.parent_session_id,
        calls: [call],
      });
    }
  }

  // Add grouped subagent sessions
  for (const [sessionId, calls] of sessionMap.entries()) {
    groups.push({
      sessionId,
      isSubAgent: true,
      parentSessionId: calls[0]?.parent_session_id,
      calls,
    });
  }

  return groups;
}
