import type { MessageSegment, ToolCallRecord } from '$lib/types';

// ---------------------------------------------------------------------------
// Display item types — used by ChatMessages to batch consecutive tool calls
// ---------------------------------------------------------------------------

export type DisplayItem =
  | { kind: 'text'; content: string }
  | { kind: 'tool-batch'; calls: ToolCallRecord[] }
  | { kind: 'subagent'; group: ToolCallGroup }
  | { kind: 'workflow'; call: ToolCallRecord };

export interface ToolCallGroup {
  sessionId: string | undefined;
  isSubAgent: boolean;
  parentSessionId: string | null | undefined;
  calls: ToolCallRecord[];
  /** For subagent groups: the agent.spawn call in the parent that spawned this session. */
  spawnCall?: ToolCallRecord;
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

  // Add grouped subagent sessions, linking each to its agent.spawn call.
  // Match: spawnCall.session_id === subagent.parent_session_id (the parent's session ID
  // is the session in which agent.spawn was called, propagated into all subagent events).
  for (const [sessionId, calls] of sessionMap.entries()) {
    const parentSessionId = calls[0]?.parent_session_id;
    const spawnCall = groups
      .filter((g) => !g.isSubAgent)
      .flatMap((g) => g.calls)
      .find((c) => c.tool_name === 'agent.spawn' && c.session_id === parentSessionId);
    groups.push({
      sessionId,
      isSubAgent: true,
      parentSessionId,
      calls,
      spawnCall,
    });
  }

  return groups;
}

/**
 * Converts message segments + resolved tool groups into a flat list of DisplayItems
 * where consecutive non-subagent tool anchors are batched together.
 * This powers the "N tool calls" collapsible grouping in ChatMessages.
 */
export function computeDisplayItems(
  segments: MessageSegment[],
  anchorExecIds: Set<string | undefined>,
  execToGroup: Map<string, ToolCallGroup>,
  spawnExecToSubagentGroup: Map<string, ToolCallGroup>,
): DisplayItem[] {
  const items: DisplayItem[] = [];
  let batch: ToolCallRecord[] | null = null;

  function flushBatch() {
    if (batch && batch.length > 0) {
      items.push({ kind: 'tool-batch', calls: batch });
      batch = null;
    }
  }

  for (const seg of segments) {
    if (seg.kind === 'text') {
      if (seg.content.trim().length > 0) {
        flushBatch();
        items.push({ kind: 'text', content: seg.content });
      }
    } else if (seg.kind === 'tool' && anchorExecIds.has(seg.execution_id)) {
      const group =
        spawnExecToSubagentGroup.get(seg.execution_id) ??
        execToGroup.get(seg.execution_id);
      if (!group) continue;

      if (group.isSubAgent) {
        flushBatch();
        items.push({ kind: 'subagent', group });
      } else if (group.calls.length === 1 && group.calls[0].tool_name === 'workflow.run') {
        flushBatch();
        items.push({ kind: 'workflow', call: group.calls[0] });
      } else {
        if (!batch) batch = [];
        batch.push(...group.calls);
      }
    }
  }

  flushBatch();
  return items;
}
