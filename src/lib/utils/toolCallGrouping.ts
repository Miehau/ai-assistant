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
  /** For subagent groups: the delegate/agent.spawn call that spawned this session. */
  spawnCall?: ToolCallRecord;
  /** Nested delegate groups spawned by calls inside this subagent. */
  childGroups?: ToolCallGroup[];
  /** Present when this group should render inside another subagent group. */
  parentSubagentSessionId?: string;
}

/**
 * Groups tool calls by session ID to enable hierarchical display.
 * Main agent tool calls are returned as individual groups.
 * Subagent tool calls are grouped by their session_id.
 */
export function groupToolCallsBySession(toolCalls: ToolCallRecord[]): ToolCallGroup[] {
  const rootGroups: ToolCallGroup[] = [];
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
      rootGroups.push({
        sessionId: call.session_id,
        isSubAgent: false,
        parentSessionId: call.parent_session_id,
        calls: [call],
      });
    }
  }

  const subagentGroups: ToolCallGroup[] = [];

  // Add grouped subagent sessions, linking each to the delegate/agent.spawn call
  // that spawned this session. Prefer the exact source_execution_id when the
  // backend provides it; fall back to the older parent-session match.
  for (const [sessionId, calls] of sessionMap.entries()) {
    const parentSessionId = calls[0]?.parent_session_id;
    const sourceExecutionId = calls.find((c) => c.source_execution_id)?.source_execution_id;
    const spawnCall =
      (sourceExecutionId
        ? toolCalls.find((c) => c.execution_id === sourceExecutionId)
        : undefined) ??
      toolCalls.find(
        (c) =>
          (c.tool_name === 'delegate' || c.tool_name === 'agent.spawn') &&
          c.session_id === parentSessionId
      );

    subagentGroups.push({
      sessionId,
      isSubAgent: true,
      parentSessionId,
      calls,
      spawnCall,
      childGroups: [],
    });
  }

  const subagentGroupBySession = new Map(
    subagentGroups.flatMap((group) => group.sessionId ? [[group.sessionId, group] as const] : [])
  );

  for (const group of subagentGroups) {
    const parentGroupSessionId = group.spawnCall?.session_id;
    const parentGroup = parentGroupSessionId
      ? subagentGroupBySession.get(parentGroupSessionId)
      : undefined;

    if (parentGroup && parentGroup.sessionId !== group.sessionId) {
      parentGroup.childGroups = [...(parentGroup.childGroups ?? []), group];
      group.parentSubagentSessionId = parentGroup.sessionId;
    }
  }

  return [
    ...rootGroups,
    ...subagentGroups.filter((group) => !group.parentSubagentSessionId),
  ];
}

export function flattenToolCallGroups(groups: ToolCallGroup[]): ToolCallGroup[] {
  const flattened: ToolCallGroup[] = [];

  function visit(group: ToolCallGroup) {
    flattened.push(group);
    for (const child of group.childGroups ?? []) {
      visit(child);
    }
  }

  for (const group of groups) {
    visit(group);
  }

  return flattened;
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
