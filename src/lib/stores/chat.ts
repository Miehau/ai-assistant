import { writable, derived, get } from 'svelte/store';
import type { Message } from '$lib/types';
import type { Model } from '$lib/types/models';
import type { SystemPrompt } from '$lib/types';
import { invoke } from '@tauri-apps/api/tauri';
import { conversationService } from '$lib/services/conversation';
import { titleGeneratorService } from '$lib/services/titleGenerator';
import { modelService, apiKeyService } from '$lib/models';
import { customBackendService } from '$lib/services/customBackendService.svelte';
import { ollamaService } from '$lib/services/ollamaService.svelte';
import { backend } from '$lib/backend/client';
import { v4 as uuidv4 } from 'uuid';
import { branchStore } from '$lib/stores/branches';
import { startAgentEventBridge } from '$lib/services/eventBridge';
import { streamMessageViaHono } from '$lib/services/honoEventBridge';
import { honoBackend } from '$lib/stores/honoBackend.svelte';
import { AGENT_EVENT_TYPES } from '$lib/types/events';
import type { AgentEvent, Attachment, ToolCallRecord, MessageSegment } from '$lib/types';
import { CustomProviderService } from '$lib/services/customProvider';
import type { CustomBackend } from '$lib/types/customBackend';
import type {
  AssistantStreamChunkPayload,
  AssistantStreamCompletedPayload,
  AssistantStreamStartedPayload,
  AgentPhaseChangedPayload,
  AgentPlanPayload,
  AgentStepCompletedPayload,
  AgentStepProposedPayload,
  AgentStepStartedPayload,
  ConversationDeletedPayload,
  ConversationUpdatedPayload,
  MessageSavedPayload,
  ToolExecutionCompletedPayload,
  ToolExecutionStartedPayload,
  ToolExecutionApprovalScope,
  ToolExecutionDecisionPayload,
  ToolExecutionProposedPayload,
  UsageUpdatedPayload,
} from '$lib/types/events';
import type { AgentPlan, AgentPlanStep, PhaseKind } from '$lib/types/agent';
import { currentConversationUsage } from '$lib/stores/tokenUsage';
import type { OllamaModel } from '$lib/types/ollama';

// Extended model type with backend name for UI display
export interface ModelWithBackend extends Model {
  backendName?: string;
}

// State stores
export const messages = writable<Message[]>([]);
export const availableModels = writable<Model[]>([]);
export const systemPrompts = writable<SystemPrompt[]>([]);
export const selectedModel = writable<string>('');
export const selectedSystemPrompt = writable<SystemPrompt | null>(null);
export const streamingEnabled = writable<boolean>(true);
export const isLoading = writable<boolean>(false);
export const attachments = writable<any[]>([]);
export const currentMessage = writable<string>('');
export const isFirstMessage = writable<boolean>(true);
export const pendingToolApprovals = writable<ToolExecutionProposedPayload[]>([]);
export const toolActivity = writable<ToolActivityEntry[]>([]);
export const agentPhase = writable<PhaseKind | null>(null);
export const agentPlan = writable<AgentPlan | null>(null);
export const agentPlanSteps = writable<AgentPlanStep[]>([]);

// Sub-agent tracking
export interface SubAgentEntry {
  agent_id: string;
  parent_agent_id: string;
  depth: number;
  task: string;
  model: string;
  started_at: number;
  status: 'running' | 'completed' | 'failed';
  result?: string;
  error?: string;
}
export const activeSubAgents = writable<SubAgentEntry[]>([]);

// Streaming-specific stores for smooth updates without array reactivity
export const streamingMessage = writable<string>('');
export const isStreaming = writable<boolean>(false);
/** How many characters of $streamingMessage have already been captured into text segments.
 *  The streaming div shows only the tail: $streamingMessage.slice($streamingSegmentedLength). */
export const streamingSegmentedLength = writable<number>(0);

// Derived stores
export const hasAttachments = derived(
  attachments,
  $attachments => $attachments.length > 0
);

// Preference keys
const PREF_LAST_USED_MODEL = 'last_used_model';
let modelsLoaded = false;
let modelsLoadingPromise: Promise<void> | null = null;
let systemPromptsLoaded = false;
let systemPromptsLoadingPromise: Promise<void> | null = null;
let stopAgentEventBridge: (() => void) | null = null;
let honoStreamController: AbortController | null = null;
let honoAgentId: string | null = null;
let streamingAssistantMessageId: string | null = null;
let streamingChunkBuffer = '';
let streamingFlushPending = false;
let pendingAssistantMessageId: string | null = null;
let requestWatchdogTimer: ReturnType<typeof setTimeout> | null = null;
const cancelledAssistantMessageIds = new Set<string>();
const TOOL_ACTIVITY_LIMIT = 8;
const toolCallsByMessageId = new Map<string, Map<string, ToolCallRecord>>();
const segmentsByMessageId = new Map<string, MessageSegment[]>();
/** Tracks how much of the accumulated streaming text has already been captured as text segments. */
let segmentedTextLength = 0;
/** Throttle buffer for AGENT_OUTPUT_DELTA — maps execution_id → latest accumulated text */
const agentOutputBuffer = new Map<string, { messageId: string; text: string }>();
let agentOutputFlushTimer: ReturnType<typeof setTimeout> | null = null;

function appendSegment(messageId: string, segment: MessageSegment) {
  const existing = segmentsByMessageId.get(messageId) ?? [];
  // Deduplicate tool segments by execution_id (e.g. approval + tool:started for same call)
  if (segment.kind === 'tool' && segment.execution_id) {
    if (existing.some((s) => s.kind === 'tool' && s.execution_id === segment.execution_id)) {
      return;
    }
  }
  const updated = [...existing, segment];
  segmentsByMessageId.set(messageId, updated);

  // Propagate all segments (text + tool) to the message immediately so
  // the interleaved rendering path shows text in the correct order relative
  // to tool bubbles during streaming. Text segments are only appended at
  // tool boundaries (infrequent), so this update is not on the hot chunk path.
  // The streaming div shows only the tail ($streamingMessage.slice(segmentedLength))
  // to avoid duplicating text that is already rendered via a text segment.
  messages.update((msgs) =>
    msgs.map((msg) =>
      msg.id === messageId ? { ...msg, segments: updated } : msg
    )
  );
}

/**
 * Flush any un-segmented streaming text as a text segment.
 * Call this BEFORE appending a tool segment so text appears in correct order.
 */
function flushTextSegment(messageId: string, fullText: string) {
  if (fullText.length > segmentedTextLength) {
    const newText = fullText.slice(segmentedTextLength);
    if (newText.trim().length > 0) {
      appendSegment(messageId, { kind: 'text', content: newText });
    }
    segmentedTextLength = fullText.length;
    streamingSegmentedLength.set(segmentedTextLength);
  }
}

function normalizeSuccess(value: unknown): boolean | undefined {
  if (value === true || value === false) return value;
  if (typeof value === 'number') {
    if (value === 1) return true;
    if (value === 0) return false;
  }
  if (typeof value === 'string') {
    const normalized = value.trim().toLowerCase();
    if (normalized === 'true' || normalized === '1') return true;
    if (normalized === 'false' || normalized === '0') return false;
  }
  return undefined;
}

export type ToolActivityEntry = {
  execution_id: string;
  tool_name: string;
  status: 'running' | 'completed' | 'failed';
  started_at: number;
  completed_at?: number;
  duration_ms?: number;
  error?: string;
};

function getToolCallsForMessage(messageId: string): ToolCallRecord[] | undefined {
  const entries = toolCallsByMessageId.get(messageId);
  if (!entries) return undefined;
  return Array.from(entries.values());
}

function upsertToolCall(
  messageId: string,
  executionId: string,
  payload: Partial<ToolCallRecord>
) {
  let entries = toolCallsByMessageId.get(messageId);
  if (!entries) {
    entries = new Map<string, ToolCallRecord>();
    toolCallsByMessageId.set(messageId, entries);
  }

  const existing = entries.get(executionId);
  const normalizedSuccess =
    payload.success === undefined ? undefined : normalizeSuccess(payload.success as unknown);
  const next: ToolCallRecord = {
    execution_id: executionId,
    tool_name: payload.tool_name ?? existing?.tool_name ?? 'unknown',
    args: payload.args ?? existing?.args ?? {},
    result: payload.result ?? existing?.result,
    success: normalizedSuccess ?? existing?.success,
    error: payload.error ?? existing?.error,
    duration_ms: payload.duration_ms ?? existing?.duration_ms,
    started_at: payload.started_at ?? existing?.started_at,
    completed_at: payload.completed_at ?? existing?.completed_at,
    session_id: payload.session_id ?? existing?.session_id,
    parent_session_id: payload.parent_session_id ?? existing?.parent_session_id,
    is_sub_agent: payload.is_sub_agent ?? existing?.is_sub_agent,
  };

  entries.set(executionId, next);
  messages.update((msgs) =>
    msgs.map((msg) =>
      msg.id === messageId ? { ...msg, tool_calls: getToolCallsForMessage(messageId) } : msg
    )
  );
}

function updateToolCallByExecutionId(
  executionId: string,
  payload: Partial<ToolCallRecord>
): boolean {
  let updated = false;
  for (const [messageId, entries] of toolCallsByMessageId.entries()) {
    if (!entries.has(executionId)) continue;
    updated = true;
    upsertToolCall(messageId, executionId, payload);
  }
  return updated;
}

function ensureAssistantMessageForToolExecution(messageId: string, timestamp: number) {
  messages.update((msgs) => {
    if (msgs.some((msg) => msg.id === messageId)) {
      return msgs;
    }

    return [
      ...msgs,
      {
        id: messageId,
        type: 'received',
        content: '',
        timestamp,
        tool_calls: getToolCallsForMessage(messageId),
      },
    ];
  });
}

function updatePlanStep(stepId: string, updates: Partial<AgentPlanStep>) {
  agentPlanSteps.update((steps) =>
    steps.map((step) => (step.id === stepId ? { ...step, ...updates } : step))
  );
}

function isNeedsHumanInputPhase(phase: unknown): boolean {
  if (!phase) return false;
  if (typeof phase === 'string') {
    return phase.toLowerCase() === 'needshumaninput' || phase.toLowerCase() === 'needs_human_input';
  }
  if (typeof phase === 'object') {
    const keys = Object.keys(phase as Record<string, unknown>);
    return keys.some((key) => key === 'NeedsHumanInput' || key === 'needs_human_input');
  }
  return false;
}

function finalizeRunningToolCalls(reason: string, timestamp: number) {
  for (const [messageId, entries] of toolCallsByMessageId.entries()) {
    for (const [executionId, entry] of entries.entries()) {
      if (entry.success !== undefined) continue;
      entries.set(executionId, {
        ...entry,
        success: false,
        error: reason,
        completed_at: timestamp,
      });
    }

    messages.update((msgs) =>
      msgs.map((msg) =>
        msg.id === messageId ? { ...msg, tool_calls: getToolCallsForMessage(messageId) } : msg
      )
    );
  }

  toolActivity.update((entries) =>
    entries.map((entry) =>
      entry.status === 'running'
        ? {
            ...entry,
            status: 'failed',
            completed_at: timestamp,
            error: reason,
          }
        : entry
    )
  );
}

function upsertToolActivityFromExecution(execution: {
  execution_id: string;
  tool_name: string;
  success: boolean;
  duration_ms: number;
  timestamp_ms: number;
  error?: string | null;
}) {
  const normalizedSuccess = normalizeSuccess(execution.success as unknown) ?? false;
  const status: ToolActivityEntry['status'] = normalizedSuccess ? 'completed' : 'failed';
  toolActivity.update((entries) => {
    let updated = false;
    const next = entries.map((entry) => {
      if (entry.execution_id !== execution.execution_id) {
        return entry;
      }
      updated = true;
      return {
        ...entry,
        tool_name: execution.tool_name,
        status,
        completed_at: execution.timestamp_ms,
        duration_ms: execution.duration_ms,
        error: execution.error ?? undefined,
      };
    });

    if (!updated) {
      next.unshift({
        execution_id: execution.execution_id,
        tool_name: execution.tool_name,
        status,
        started_at: execution.timestamp_ms,
        completed_at: execution.timestamp_ms,
        duration_ms: execution.duration_ms,
        error: execution.error ?? undefined,
      });
    }

    return next.slice(0, TOOL_ACTIVITY_LIMIT);
  });
}

function isAgentErrorContent(content: string): boolean {
  const trimmed = content.trimStart();
  return trimmed.startsWith('Agent error:') || trimmed.startsWith('Agent setup error:');
}

function flushStreamingChunks() {
  if (!streamingChunkBuffer) {
    streamingFlushPending = false;
    return;
  }

  const chunk = streamingChunkBuffer;
  streamingChunkBuffer = '';
  streamingMessage.update((content) => content + chunk);
  streamingFlushPending = false;
}

function clearRequestWatchdog() {
  if (requestWatchdogTimer !== null) {
    clearTimeout(requestWatchdogTimer);
    requestWatchdogTimer = null;
  }
}

function startRequestWatchdog(messageId: string) {
  clearRequestWatchdog();
  requestWatchdogTimer = setTimeout(() => {
    const isStillPending =
      pendingAssistantMessageId === messageId || streamingAssistantMessageId === messageId;
    if (!isStillPending) {
      return;
    }

    console.error('Agent request watchdog timeout for message:', messageId);
    cancelledAssistantMessageIds.delete(messageId);
    resetStreamingState();
  }, 180_000);
}

function resetStreamingState() {
  clearRequestWatchdog();
  streamingAssistantMessageId = null;
  pendingAssistantMessageId = null;
  honoAgentId = null;
  isStreaming.set(false);
  streamingMessage.set('');
  streamingChunkBuffer = '';
  streamingFlushPending = false;
  segmentedTextLength = 0;
  streamingSegmentedLength.set(0);
  isLoading.set(false);
  // Flush any pending agent output deltas before clearing
  if (agentOutputFlushTimer) {
    clearTimeout(agentOutputFlushTimer);
    agentOutputFlushTimer = null;
    for (const [execId, entry] of agentOutputBuffer) {
      upsertToolCall(entry.messageId, execId, { result: entry.text });
    }
  }
  agentOutputBuffer.clear();
}

function handleAgentEvent(event: AgentEvent) {
  if (event.event_type === AGENT_EVENT_TYPES.MESSAGE_SAVED) {
      const payload = event.payload as MessageSavedPayload;
      if (payload.role !== 'user' && cancelledAssistantMessageIds.has(payload.message_id)) {
        return;
      }
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      const isAssistant = payload.role !== 'user';
      if (isAssistant && payload.tool_executions && payload.tool_executions.length > 0) {
        for (const execution of payload.tool_executions) {
          const normalizedSuccess = normalizeSuccess(execution.success as unknown);
          upsertToolCall(payload.message_id, execution.id, {
            tool_name: execution.tool_name,
            args: execution.parameters ?? {},
            result: execution.result,
            success: normalizedSuccess ?? execution.success,
            error: execution.error ?? undefined,
            duration_ms: execution.duration_ms,
            completed_at: execution.timestamp_ms,
          });

          upsertToolActivityFromExecution({
            execution_id: execution.id,
            tool_name: execution.tool_name,
            success: normalizedSuccess ?? execution.success,
            duration_ms: execution.duration_ms,
            timestamp_ms: execution.timestamp_ms,
            error: execution.error,
          });
        }
      }

      const attachments: Attachment[] = payload.attachments.map((attachment) => ({
        name: attachment.name,
        data: attachment.data,
        attachment_type: attachment.attachment_type as Attachment['attachment_type'],
        description: attachment.description,
        transcript: attachment.transcript,
      }));
      const toolCalls = isAssistant ? getToolCallsForMessage(payload.message_id) : undefined;
      const hasToolCalls = Boolean(toolCalls && toolCalls.length > 0);
      const hasContent = payload.content.trim().length > 0;
      const hasAttachments = attachments.length > 0;
      const shouldStoreAssistantMessage = hasContent || hasToolCalls || hasAttachments;
      if (isAssistant && !shouldStoreAssistantMessage) {
        messages.update((msgs) => msgs.filter((msg) => msg.id !== payload.message_id));
        return;
      }

      const content = payload.content;

      messages.update((msgs) => {
        const existingIndex = msgs.findIndex((msg) => msg.id === payload.message_id);
        const newMessage: Message = {
          id: payload.message_id,
          type: isAssistant ? 'received' : 'sent',
          content,
          attachments: attachments.length ? attachments : undefined,
          timestamp: payload.timestamp_ms,
          tool_calls: isAssistant ? toolCalls : undefined,
          isError: isAssistant ? isAgentErrorContent(content) : undefined,
        };

        if (existingIndex === -1) {
          return [...msgs, newMessage];
        }

        const existing = msgs[existingIndex];
        const updated: Message = {
          ...existing,
          ...newMessage,
          attachments: newMessage.attachments ?? existing.attachments,
          tool_calls: isAssistant ? toolCalls ?? existing.tool_calls : existing.tool_calls,
        };
        const next = [...msgs];
        next[existingIndex] = updated;
        return next;
      });
    }

    if (event.event_type === AGENT_EVENT_TYPES.USAGE_UPDATED) {
      const payload = event.payload as UsageUpdatedPayload;
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      currentConversationUsage.set({
        conversation_id: payload.conversation_id,
        total_prompt_tokens: payload.total_prompt_tokens,
        total_completion_tokens: payload.total_completion_tokens,
        total_tokens: payload.total_tokens,
        total_cost: payload.total_cost,
        message_count: payload.message_count,
        last_updated: new Date(payload.timestamp_ms).toISOString(),
      });
    }

    if (event.event_type === AGENT_EVENT_TYPES.CONVERSATION_UPDATED) {
      const payload = event.payload as ConversationUpdatedPayload;
      conversationService.applyConversationUpdate(
        payload.conversation_id,
        payload.name
      );
    }

    if (event.event_type === AGENT_EVENT_TYPES.CONVERSATION_DELETED) {
      const payload = event.payload as ConversationDeletedPayload;
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      conversationService.applyConversationDeleted(payload.conversation_id);
      honoBackend.removeSession(payload.conversation_id);
      messages.set([]);
      isFirstMessage.set(true);
      resetStreamingState();
      pendingToolApprovals.set([]);
      toolActivity.set([]);
      toolCallsByMessageId.clear();
      segmentsByMessageId.clear();
      cancelledAssistantMessageIds.clear();
      agentPhase.set(null);
      agentPlan.set(null);
      agentPlanSteps.set([]);
    }

    if (event.event_type === AGENT_EVENT_TYPES.ASSISTANT_STREAM_STARTED) {
      const payload = event.payload as AssistantStreamStartedPayload;
      if (cancelledAssistantMessageIds.has(payload.message_id)) {
        return;
      }
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      streamingAssistantMessageId = payload.message_id;
      isStreaming.set(true);
      streamingMessage.set('');
      isLoading.set(true);
    }

    if (event.event_type === AGENT_EVENT_TYPES.ASSISTANT_STREAM_CHUNK) {
      const payload = event.payload as AssistantStreamChunkPayload;
      if (cancelledAssistantMessageIds.has(payload.message_id)) {
        return;
      }
      if (streamingAssistantMessageId !== payload.message_id) {
        return;
      }
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      streamingChunkBuffer += payload.chunk;

      if (!streamingFlushPending) {
        streamingFlushPending = true;
        if (typeof window !== 'undefined' && 'requestAnimationFrame' in window) {
          window.requestAnimationFrame(() => flushStreamingChunks());
        } else {
          flushStreamingChunks();
        }
      }
    }

    if (event.event_type === AGENT_EVENT_TYPES.ASSISTANT_STREAM_COMPLETED) {
      const payload = event.payload as AssistantStreamCompletedPayload;
      const isCurrentMessage =
        streamingAssistantMessageId === payload.message_id ||
        pendingAssistantMessageId === payload.message_id;
      if (!isCurrentMessage && !cancelledAssistantMessageIds.has(payload.message_id)) {
        return;
      }

      if (cancelledAssistantMessageIds.has(payload.message_id)) {
        cancelledAssistantMessageIds.delete(payload.message_id);
        if (isCurrentMessage) {
          resetStreamingState();
        }
        return;
      }

      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      const toolCalls = getToolCallsForMessage(payload.message_id);
      const hasToolCalls = Boolean(toolCalls && toolCalls.length > 0);
      const hasContent = payload.content.trim().length > 0;

      // Capture any remaining un-segmented text (text after the last tool call)
      if (hasContent) {
        flushTextSegment(payload.message_id, payload.content);
      }
      const segments = segmentsByMessageId.get(payload.message_id);
      const hasSegments = segments && segments.length > 0;

      if (!hasContent && !hasToolCalls) {
        messages.update((msgs) => msgs.filter((msg) => msg.id !== payload.message_id));
      } else {
        const content = payload.content;
        messages.update((msgs) => {
          const existingIndex = msgs.findIndex((msg) => msg.id === payload.message_id);
          const newMessage: Message = {
            id: payload.message_id,
            type: 'received',
            content,
            timestamp: payload.timestamp_ms,
            tool_calls: toolCalls,
            isError: isAgentErrorContent(content),
            segments: hasSegments ? segments : undefined,
          };

          if (existingIndex === -1) {
            return [...msgs, newMessage];
          }

          const existing = msgs[existingIndex];
          const updated: Message = {
            ...existing,
            ...newMessage,
            attachments: existing.attachments,
            tool_calls: toolCalls ?? existing.tool_calls,
            segments: hasSegments ? segments : existing.segments,
          };
          const next = [...msgs];
          next[existingIndex] = updated;
          return next;
        });
      }

      resetStreamingState();
    }

    if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED) {
      const payload = event.payload as ToolExecutionStartedPayload;
      if (payload.message_id && cancelledAssistantMessageIds.has(payload.message_id)) {
        return;
      }
      const currentConversation = conversationService.getCurrentConversation();
      if (payload.conversation_id && currentConversation?.id !== payload.conversation_id) {
        return;
      }

      if (payload.message_id) {
        ensureAssistantMessageForToolExecution(payload.message_id, payload.timestamp_ms);
        upsertToolCall(payload.message_id, payload.execution_id, {
          tool_name: payload.tool_name,
          args: payload.args ?? {},
          started_at: payload.timestamp_ms,
          session_id: payload.session_id,
          parent_session_id: payload.parent_session_id,
          is_sub_agent: payload.is_sub_agent,
        });
        // Flush any accumulated streaming text as a text segment BEFORE the tool segment
        // so the interleaved order is preserved: text → tool → text → tool → ...
        const accumulatedText = get(streamingMessage) + streamingChunkBuffer;
        flushTextSegment(payload.message_id, accumulatedText);
        appendSegment(payload.message_id, { kind: 'tool', execution_id: payload.execution_id });
      }

      toolActivity.update((entries) => {
        const existingIndex = entries.findIndex(
          (entry) => entry.execution_id === payload.execution_id
        );
        if (existingIndex !== -1) {
          const next = [...entries];
          next[existingIndex] = {
            ...next[existingIndex],
            tool_name: payload.tool_name,
            status: 'running',
            started_at: payload.timestamp_ms,
            completed_at: undefined,
            duration_ms: undefined,
            error: undefined,
          };
          return next.slice(0, TOOL_ACTIVITY_LIMIT);
        }

        const nextEntry: ToolActivityEntry = {
          execution_id: payload.execution_id,
          tool_name: payload.tool_name,
          status: 'running',
          started_at: payload.timestamp_ms,
        };

        return [nextEntry, ...entries].slice(0, TOOL_ACTIVITY_LIMIT);
      });
    }

    if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_COMPLETED) {
      const payload = event.payload as ToolExecutionCompletedPayload;
      if (payload.message_id && cancelledAssistantMessageIds.has(payload.message_id)) {
        return;
      }
      const currentConversation = conversationService.getCurrentConversation();
      if (payload.conversation_id && currentConversation?.id !== payload.conversation_id) {
        return;
      }

      const normalizedSuccess = normalizeSuccess(payload.success as unknown);
      const completionUpdate: Partial<ToolCallRecord> = {
        tool_name: payload.tool_name,
        result: payload.result,
        error: payload.error,
        duration_ms: payload.duration_ms,
        completed_at: payload.timestamp_ms,
        session_id: payload.session_id,
        parent_session_id: payload.parent_session_id,
        is_sub_agent: payload.is_sub_agent,
        ...(normalizedSuccess !== undefined ? { success: normalizedSuccess } : {}),
      };

      if (payload.message_id) {
        ensureAssistantMessageForToolExecution(payload.message_id, payload.timestamp_ms);
        upsertToolCall(payload.message_id, payload.execution_id, completionUpdate);
      }

      toolActivity.update((entries) => {
        const status: ToolActivityEntry['status'] =
          normalizedSuccess === undefined ? 'failed' : normalizedSuccess ? 'completed' : 'failed';
        let updated = false;
        const next = entries.map((entry) => {
          if (entry.execution_id !== payload.execution_id) {
            return entry;
          }
          updated = true;
          return {
            ...entry,
            status,
            completed_at: payload.timestamp_ms,
            duration_ms: payload.duration_ms,
            error: payload.error,
          };
        });

        if (!updated) {
          next.unshift({
            execution_id: payload.execution_id,
            tool_name: payload.tool_name,
            status,
            started_at: payload.timestamp_ms,
            completed_at: payload.timestamp_ms,
            duration_ms: payload.duration_ms,
            error: payload.error,
          });
        }

        return next.slice(0, TOOL_ACTIVITY_LIMIT);
      });

      updateToolCallByExecutionId(payload.execution_id, completionUpdate);
    }

    // Child agent streaming text → update the delegate tool call's result as live preview.
    // Throttled: accumulate text locally and flush to the store at most every 150ms
    // to avoid flooding the reactive graph with per-token updates.
    if (event.event_type === AGENT_EVENT_TYPES.AGENT_OUTPUT_DELTA) {
      const payload = event.payload as import('$lib/types/events').AgentOutputDeltaPayload;
      if (payload.message_id) {
        agentOutputBuffer.set(payload.parent_execution_id, {
          messageId: payload.message_id,
          text: payload.text,
        });
        if (!agentOutputFlushTimer) {
          agentOutputFlushTimer = setTimeout(() => {
            agentOutputFlushTimer = null;
            for (const [execId, entry] of agentOutputBuffer) {
              upsertToolCall(entry.messageId, execId, { result: entry.text });
            }
          }, 150);
        }
      }
    }

    if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_PROPOSED) {
      const payload = event.payload as ToolExecutionProposedPayload;
      if (payload.message_id && cancelledAssistantMessageIds.has(payload.message_id)) {
        console.log('[chat] TOOL_EXECUTION_PROPOSED skipped — message cancelled');
        return;
      }
      const currentConversation = conversationService.getCurrentConversation();
      if (payload.conversation_id && currentConversation?.id !== payload.conversation_id) {
        console.warn('[chat] TOOL_EXECUTION_PROPOSED conversation mismatch', {
          payloadConvId: payload.conversation_id,
          currentConvId: currentConversation?.id,
        });
        return;
      }

      console.log('[chat] TOOL_EXECUTION_PROPOSED queued:', payload.tool_name, payload.approval_id);
      pendingToolApprovals.update((approvals) => {
        if (approvals.some((entry) => entry.approval_id === payload.approval_id)) {
          return approvals;
        }
        return [...approvals, payload];
      });
    }

    if (
      event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_APPROVED ||
      event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_DENIED
    ) {
      const payload = event.payload as ToolExecutionDecisionPayload;
      pendingToolApprovals.update((approvals) =>
        approvals.filter((entry) => entry.approval_id !== payload.approval_id)
      );

      // Mark denied tool calls so the UI can show them as denied (amber)
      if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_DENIED) {
        const msgId = (payload as Record<string, unknown>).message_id as string | undefined;
        if (msgId) {
          upsertToolCall(msgId, payload.execution_id, {
            tool_name: payload.tool_name,
            success: false,
            error: 'Tool execution denied by user',
            completed_at: Date.now(),
          });
        }
      }
    }

    if (event.event_type === AGENT_EVENT_TYPES.AGENT_PHASE_CHANGED) {
      const payload = event.payload as AgentPhaseChangedPayload;
      agentPhase.set(payload.phase as PhaseKind);
      if (isNeedsHumanInputPhase(payload.phase)) {
        // Capture any in-flight streaming text as a text segment before clearing
        // the streaming state. Without this the text is lost when the agent pauses
        // for an approval and ASSISTANT_STREAM_COMPLETED never fires.
        if (streamingAssistantMessageId) {
          const accumulatedText = get(streamingMessage) + streamingChunkBuffer;
          streamingChunkBuffer = '';
          flushTextSegment(streamingAssistantMessageId, accumulatedText);
        }
        finalizeRunningToolCalls('Awaiting user input', Date.now());
        isLoading.set(false);
        isStreaming.set(false);
        streamingMessage.set('');
        streamingAssistantMessageId = null;
        pendingAssistantMessageId = null;
      }
    }

    if (
      event.event_type === AGENT_EVENT_TYPES.AGENT_PLAN_CREATED ||
      event.event_type === AGENT_EVENT_TYPES.AGENT_PLAN_ADJUSTED
    ) {
      const payload = event.payload as AgentPlanPayload;
      const plan = payload.plan as AgentPlan;
      agentPlan.set(plan);
      agentPlanSteps.set(plan?.steps || []);
    }

    if (event.event_type === AGENT_EVENT_TYPES.AGENT_STEP_PROPOSED) {
      const payload = event.payload as AgentStepProposedPayload;
      const step = payload.step as AgentPlanStep;
      if (step?.id) {
        updatePlanStep(step.id, { status: step.status });
      }
    }

    if (event.event_type === AGENT_EVENT_TYPES.AGENT_STEP_STARTED) {
      const payload = event.payload as AgentStepStartedPayload;
      updatePlanStep(payload.step_id, { status: 'Executing' });
    }

    if (event.event_type === AGENT_EVENT_TYPES.AGENT_STEP_COMPLETED) {
      const payload = event.payload as AgentStepCompletedPayload;
      updatePlanStep(payload.step_id, { status: payload.success ? 'Completed' : 'Failed' });
    }

}

export async function startAgentEvents() {
  if (stopAgentEventBridge) return;

  try {
    const currentConversation = conversationService.getCurrentConversation();
    const pendingApprovals = await backend.listPendingToolApprovals();
    pendingToolApprovals.set(
      pendingApprovals.filter((approval) => {
        if (!approval.conversation_id) {
          return true;
        }
        return currentConversation?.id === approval.conversation_id;
      })
    );
  } catch (error) {
    console.error('Failed to load pending tool approvals:', error);
  }

  stopAgentEventBridge = await startAgentEventBridge(handleAgentEvent);
}

// Actions
export async function loadModels(options: { force?: boolean } = {}) {
  const { force = false } = options;
  if (modelsLoadingPromise) {
    await modelsLoadingPromise;
    if (!force && modelsLoaded) return;
  }
  if (modelsLoaded && !force) return;

  const loader = (async () => {
    try {
      console.log('[ChatStore] Starting loadModels...');

      // First load API keys to ensure model availability is updated
      console.log('[ChatStore] Loading API keys...');
      await apiKeyService.loadAllApiKeys();
      console.log('[ChatStore] API keys loaded');

      // Load custom backends for custom model support
      console.log('[ChatStore] Loading custom backends...');
      await customBackendService.loadBackends();
      console.log('[ChatStore] Custom backends count:', customBackendService.backends.length);

      // Get models from both sources
      console.log('[ChatStore] Loading stored models...');
      const storedModels = await modelService.loadModels();
      console.log('[ChatStore] Stored models count:', storedModels.length);

      console.log('[ChatStore] Getting registry models with capabilities...');
      const registryModels = modelService.getAvailableModelsWithCapabilities();
      console.log('[ChatStore] Registry models count:', registryModels.length);

      // Combine models, prioritizing registry models for their capabilities
      const combinedModels: ModelWithBackend[] = [...storedModels];

      // Add registry models that aren't already in stored models
      for (const regModel of registryModels) {
        const existingIndex = combinedModels.findIndex(
          m => m.model_name === regModel.model_name && m.provider === regModel.provider
        );

        if (existingIndex >= 0) {
          // Update existing model with capabilities and specs
          combinedModels[existingIndex] = {
            ...combinedModels[existingIndex],
            capabilities: regModel.capabilities,
            specs: regModel.specs
          };
        } else {
          // Add new model from registry
          combinedModels.push(regModel);
        }
      }

      // Convert custom backends directly into model entries
      // Each backend becomes a selectable "model" in the chat
      const customBackendModels: ModelWithBackend[] = customBackendService.backends.map(backend => ({
        provider: 'custom',
        model_name: backend.name,  // Use backend name as model identifier
        name: backend.name,
        enabled: true,
        custom_backend_id: backend.id,
        backendName: backend.name,
      }));

      // Add custom backend models to the list
      combinedModels.push(...customBackendModels);

      // In server (Hono) mode, also load models registered in the server DB
      if (honoBackend.enabled) {
        try {
          const serverModels = await honoBackend.getClient().listModels();
          for (const sm of serverModels) {
            const alreadyPresent = combinedModels.some(
              m => m.model_name === sm.name && m.provider === sm.provider,
            );
            if (!alreadyPresent) {
              combinedModels.push({
                provider: sm.provider,
                model_name: sm.name,
                name: sm.displayName ?? sm.name,
                enabled: true,
              });
            }
          }
        } catch (e) {
          console.warn('[ChatStore] Failed to load server models:', e);
        }
      }

      console.log('[ChatStore] Combined models count:', combinedModels.length);
      console.log('[ChatStore] Custom backend models:', customBackendModels.map(m => m.name));

      // Filter to only enabled models
      const enabledModels = combinedModels.filter(model => model.enabled);

      console.log('[ChatStore] Enabled models count:', enabledModels.length);
      console.log('[ChatStore] Enabled models:', enabledModels.map(m => `${m.model_name} (${m.provider})`));

      availableModels.set(enabledModels);

      // Try to restore last used model
      const lastUsedModel = await getLastUsedModel();
      const modelToSelect = lastUsedModel && enabledModels.some(m => m.model_name === lastUsedModel)
        ? lastUsedModel
        : enabledModels[0]?.model_name || null;

      if (modelToSelect) {
        selectedModel.set(modelToSelect);
        console.log('[ChatStore] Selected model:', modelToSelect, lastUsedModel ? '(restored from preferences)' : '(default)');
      } else {
        console.warn('[ChatStore] No enabled models available!');
      }

      // Fire-and-forget Ollama discovery and merge into available models
      void ollamaService.discoverModels().then((models) => {
        mergeOllamaModels(models, lastUsedModel);
        if (lastUsedModel && models.some((model) => model.name === lastUsedModel)) {
          if (modelToSelect && get(selectedModel) === modelToSelect) {
            selectedModel.set(lastUsedModel);
          }
        }
      });

      modelsLoaded = true;
    } catch (error) {
      modelsLoaded = false;
      console.error('[ChatStore] Failed to load models:', error);
    }
  })();

  modelsLoadingPromise = loader;
  try {
    await loader;
  } finally {
    if (modelsLoadingPromise === loader) {
      modelsLoadingPromise = null;
    }
  }
}

function mergeOllamaModels(models: OllamaModel[], lastUsedModel?: string | null) {
  const currentModels = get(availableModels);
  const nonOllamaModels = currentModels.filter(model => model.provider !== 'ollama');
  const ollamaModels: ModelWithBackend[] = models.map(model => ({
    provider: 'ollama',
    model_name: model.name,
    name: model.name,
    enabled: true,
  }));

  const nextModels = [...nonOllamaModels, ...ollamaModels].filter(model => model.enabled);
  availableModels.set(nextModels);

  if (!get(selectedModel)) {
    const nextSelection =
      (lastUsedModel && ollamaModels.some(model => model.model_name === lastUsedModel))
        ? lastUsedModel
        : ollamaModels[0]?.model_name;
    if (nextSelection) {
      selectedModel.set(nextSelection);
    }
  }
}

// Get the last used model from preferences
async function getLastUsedModel(): Promise<string | null> {
  try {
    const result = await invoke<string | null>('get_preference', { key: PREF_LAST_USED_MODEL });
    return result;
  } catch (error) {
    console.error('[ChatStore] Failed to get last used model:', error);
    return null;
  }
}

// Save the last used model to preferences
export async function saveLastUsedModel(modelName: string): Promise<void> {
  try {
    await invoke('set_preference', { key: PREF_LAST_USED_MODEL, value: modelName });
    console.log('[ChatStore] Saved last used model:', modelName);
  } catch (error) {
    console.error('[ChatStore] Failed to save last used model:', error);
  }
}

export async function loadSystemPrompts(options: { force?: boolean } = {}) {
  const { force = false } = options;
  if (systemPromptsLoadingPromise) {
    await systemPromptsLoadingPromise;
    if (!force && systemPromptsLoaded) return;
  }
  if (systemPromptsLoaded && !force) return;

  const loader = (async () => {
    try {
      const prompts = await invoke<SystemPrompt[]>('get_all_system_prompts');
      systemPrompts.set(prompts);

      if (prompts.length > 0) {
        selectedSystemPrompt.set(prompts[0]);
      }
      systemPromptsLoaded = true;
    } catch (error) {
      systemPromptsLoaded = false;
      console.error('Failed to load system prompts:', error);
    }
  })();

  systemPromptsLoadingPromise = loader;
  try {
    await loader;
  } finally {
    if (systemPromptsLoadingPromise === loader) {
      systemPromptsLoadingPromise = null;
    }
  }
}

function honoItemsToMessages(
  items: import('$lib/backend/http-client').SessionItem[],
  agents: import('$lib/backend/http-client').AgentStatusResponse[] = [],
): Message[] {
  // Build agent map for hierarchy lookups
  const agentMap = new Map<string, import('$lib/backend/http-client').AgentStatusResponse>();
  for (const a of agents) agentMap.set(a.id, a);

  // sourceCallId → subAgentId: lets us attach subagent tool calls right after agent.spawn
  const subagentBySourceCallId = new Map<string, string>();
  for (const a of agents) {
    if (a.sourceCallId && a.parentId) subagentBySourceCallId.set(a.sourceCallId, a.id);
  }

  // Build output map for all items (root + subagent)
  const outputByCallId = new Map<string, import('$lib/backend/http-client').SessionItem>();
  for (const item of items) {
    if (item.type === 'function_call_output' && item.callId) {
      outputByCallId.set(item.callId, item);
    }
  }

  // Group subagent function_call items by agentId (in sequence order, preserved by listBySession)
  const subagentCallsByAgentId = new Map<string, ToolCallRecord[]>();
  for (const item of items) {
    if (item.type !== 'function_call') continue;
    const agent = item.agentId ? agentMap.get(item.agentId) : undefined;
    if (!agent?.parentId) continue; // root agent calls handled in main loop

    let args: Record<string, unknown> = {};
    try { args = item.arguments ? JSON.parse(item.arguments) : {}; } catch { /* ignore */ }
    const out = item.callId ? outputByCallId.get(item.callId) : undefined;

    const calls = subagentCallsByAgentId.get(item.agentId!) ?? [];
    calls.push({
      execution_id: item.callId ?? item.id,
      tool_name: item.name ?? 'unknown',
      args,
      result: out?.output ?? undefined,
      success: out ? !out.isError : undefined,
      error: out?.isError ? (out.output ?? undefined) : undefined,
      duration_ms: item.durationMs ?? undefined,
      session_id: item.agentId ?? undefined,
      parent_session_id: agent.parentId,
      is_sub_agent: true,
    });
    subagentCallsByAgentId.set(item.agentId!, calls);
  }

  // Process root-agent items to build messages; splice in subagent calls after agent.spawn
  const result: Message[] = [];
  const pendingToolCalls: ToolCallRecord[] = [];

  for (const item of items) {
    const agent = item.agentId ? agentMap.get(item.agentId) : undefined;
    // Skip subagent items — they've been collected above
    if (agent?.parentId) continue;

    if (item.type === 'function_call') {
      let args: Record<string, unknown> = {};
      try { args = item.arguments ? JSON.parse(item.arguments) : {}; } catch { /* ignore */ }
      const out = item.callId ? outputByCallId.get(item.callId) : undefined;

      pendingToolCalls.push({
        execution_id: item.callId ?? item.id,
        tool_name: item.name ?? 'unknown',
        args,
        result: out?.output ?? undefined,
        success: out ? !out.isError : undefined,
        error: out?.isError ? (out.output ?? undefined) : undefined,
        duration_ms: item.durationMs ?? undefined,
        session_id: item.agentId ?? undefined,
        is_sub_agent: false,
      });

      // Splice subagent tool calls right after the agent.spawn that spawned them
      if (item.callId && item.name === 'agent.spawn') {
        const subAgentId = subagentBySourceCallId.get(item.callId);
        if (subAgentId) {
          const subCalls = subagentCallsByAgentId.get(subAgentId) ?? [];
          pendingToolCalls.push(...subCalls);
        }
      }
    } else if (item.type === 'message') {
      if (item.role === 'user') {
        result.push({ id: item.id, type: 'sent', content: item.content ?? '' });
      } else if (item.role === 'assistant') {
        const toolCalls = pendingToolCalls.length > 0 ? [...pendingToolCalls] : undefined;
        pendingToolCalls.length = 0;
        result.push({ id: item.id, type: 'received', content: item.content ?? '', tool_calls: toolCalls });
      }
    }
    // skip 'system', 'reasoning', 'function_call_output' (handled above)
  }

  // trailing tool calls with no final assistant message (e.g. agent still waiting)
  if (pendingToolCalls.length > 0) {
    result.push({ id: uuidv4(), type: 'received', content: '', tool_calls: [...pendingToolCalls] });
  }

  return result;
}

export async function loadConversationHistory(conversationId: string) {
  try {
    let loadedMessages: Message[];

    if (honoBackend.enabled) {
      const sessionId = honoBackend.getSessionId(conversationId);
      if (sessionId) {
        const session = await honoBackend.getClient().getSession(sessionId);
        loadedMessages = honoItemsToMessages(session.items ?? [], session.agents ?? []);
      } else {
        loadedMessages = [];
      }
    } else {
      loadedMessages = await conversationService.getDisplayHistory(conversationId);
    }

    // Sync toolCallsByMessageId with the loaded history so that any future
    // upsertToolCall calls do not overwrite the correct completed/failed state
    // with undefined (which happens when the map has no entry for a message ID).
    toolCallsByMessageId.clear();
    for (const msg of loadedMessages) {
      if (msg.tool_calls && msg.tool_calls.length > 0) {
        const entries = new Map<string, ToolCallRecord>();
        for (const tc of msg.tool_calls) {
          entries.set(tc.execution_id, tc);
        }
        toolCallsByMessageId.set(msg.id, entries);
      }
    }

    messages.set(loadedMessages);

    // If there are messages, this is not a first message scenario
    if (loadedMessages.length > 0) {
      isFirstMessage.set(false);
    }
  } catch (error) {
    console.error('Failed to load conversation history:', error);
  }
}

export function toggleStreaming() {
  streamingEnabled.update(value => !value);
}

// Helper to generate unique message IDs using UUID v4
function generateMessageId(): string {
  return uuidv4();
}

function handleTsServerEvent(
  event: { type: string; [key: string]: unknown },
  assistantMessageId: string,
) {
  const now = Date.now();

  switch (event.type) {
    case 'tool:started': {
      const callId = String(event.callId ?? '');
      const toolName = String(event.name ?? 'unknown');
      const parentId = event.parentId as string | null;

      ensureAssistantMessageForToolExecution(assistantMessageId, now);
      upsertToolCall(assistantMessageId, callId, {
        tool_name: toolName,
        args: (event.args as Record<string, unknown>) ?? {},
        started_at: now,
        session_id: String(event.agentId ?? ''),
        parent_session_id: parentId,
        is_sub_agent: parentId != null,
      });

      toolActivity.update(entries => {
        const nextEntry: ToolActivityEntry = {
          execution_id: callId,
          tool_name: toolName,
          status: 'running',
          started_at: now,
        };
        return [nextEntry, ...entries].slice(0, TOOL_ACTIVITY_LIMIT);
      });
      break;
    }

    case 'tool:completed': {
      const callId = String(event.callId ?? '');
      const toolName = String(event.name ?? 'unknown');
      const ok = event.ok as boolean;
      const parentId = event.parentId as string | null;
      const durationMs = (event.durationMs as number) ?? 0;

      upsertToolCall(assistantMessageId, callId, {
        tool_name: toolName,
        success: ok,
        completed_at: now,
        duration_ms: durationMs,
        session_id: String(event.agentId ?? ''),
        parent_session_id: parentId,
        is_sub_agent: parentId != null,
      });

      toolActivity.update(entries => {
        const status = ok ? 'completed' : 'failed';
        let updated = false;
        const next = entries.map(entry => {
          if (entry.execution_id !== callId) return entry;
          updated = true;
          return {
            ...entry,
            status: status as ToolActivityEntry['status'],
            completed_at: now,
            duration_ms: durationMs,
          };
        });
        if (!updated) {
          next.unshift({
            execution_id: callId,
            tool_name: toolName,
            status: status as ToolActivityEntry['status'],
            started_at: now,
            completed_at: now,
            duration_ms: durationMs,
          });
        }
        return next.slice(0, TOOL_ACTIVITY_LIMIT);
      });
      break;
    }

    case 'tool:proposed': {
      console.log('[TS Server] Tool proposed:', event);
      break;
    }

    case 'agent:started': {
      console.log('[TS Server] Agent started:', event.agentId, 'depth:', event.depth);
      break;
    }

    case 'agent:completed':
    case 'agent:failed': {
      console.log('[TS Server] Agent done:', event.type, event.agentId);
      break;
    }
  }
}

async function sendMessageViaCustomBackend(
  content: string,
  systemPrompt: string,
  customBackend: CustomBackend,
  modelName: string,
  assistantMessageId: string,
  userMessageId: string,
) {
  const customService = CustomProviderService.fromBackend(customBackend);

  // Add user message to the store
  const userMessage: Message = {
    id: userMessageId,
    type: 'sent',
    content,
    timestamp: Date.now(),
  };
  messages.update(msgs => [...msgs, userMessage]);

  // Set up streaming state
  streamingAssistantMessageId = assistantMessageId;
  isStreaming.set(true);
  streamingMessage.set('');

  // Build conversation history in OpenAI format
  const existingMessages = get(messages);
  const formattedMessages = [
    { role: 'system' as const, content: systemPrompt },
    ...existingMessages
      .filter(m => m.id !== userMessageId)
      .map(m => ({
        role: m.type === 'sent' ? 'user' as const : 'assistant' as const,
        content: m.content,
      })),
    { role: 'user' as const, content },
  ];

  try {
    const result = await customService.createChatCompletion(
      modelName,
      customBackend.url,
      formattedMessages,
      true, // stream
      (chunk: string) => {
        streamingChunkBuffer += chunk;
        if (!streamingFlushPending) {
          streamingFlushPending = true;
          if (typeof window !== 'undefined' && 'requestAnimationFrame' in window) {
            window.requestAnimationFrame(() => flushStreamingChunks());
          } else {
            flushStreamingChunks();
          }
        }
      },
      AbortSignal.timeout(180_000),
      (event: { type: string; [key: string]: unknown }) => {
        handleTsServerEvent(event, assistantMessageId);
      },
    );

    // Finalize the message
    const finalContent = get(streamingMessage) || result.content;
    messages.update(msgs => {
      const existing = msgs.findIndex(m => m.id === assistantMessageId);
      const newMsg: Message = {
        id: assistantMessageId,
        type: 'received',
        content: finalContent,
        timestamp: Date.now(),
        tool_calls: getToolCallsForMessage(assistantMessageId),
      };
      if (existing !== -1) {
        const next = [...msgs];
        next[existing] = newMsg;
        return next;
      }
      return [...msgs, newMsg];
    });
  } finally {
    resetStreamingState();
  }
}

export async function sendMessage() {
  if (get(isLoading)) return;

  // Get current values from stores using get() instead of subscribe
  const currentMessageValue = get(currentMessage);
  const attachmentsValue = [...get(attachments)];
  const selectedModelValue = get(selectedModel);
  const selectedSystemPromptValue = get(selectedSystemPrompt);
  const isFirstMessageValue = get(isFirstMessage);
  const streamingEnabledValue = get(streamingEnabled);

  if (!currentMessageValue.trim() && attachmentsValue.length === 0) return;

  isLoading.set(true);

  try {
    const models = get(availableModels);
    let selectedModelObject = models.find(m => m.model_name === selectedModelValue);
    if (!selectedModelObject) {
      console.warn(`[ChatStore] Selected model missing: ${selectedModelValue}, falling back to first available model`);
      selectedModelObject = models[0];
      if (!selectedModelObject) {
        throw new Error('No available models to send message');
      }
      selectedModel.set(selectedModelObject.model_name);
    }

    // Clear input fields
    currentMessage.set('');
    attachments.set([]);

    // Default system prompt
    const defaultSystemPrompt =
      'You are a helpful assistant. Before saying you cannot do something, consider what you can do with the available tools and attempt that first.';

    // Get system prompt content safely
    let systemPromptContent = defaultSystemPrompt;
    if (selectedSystemPromptValue) {
      // Use type assertion to avoid TypeScript error
      const prompt = selectedSystemPromptValue as any;
      systemPromptContent = prompt.content || defaultSystemPrompt;
    }

    // Get or create the current conversation
    const currentConversation = conversationService.getCurrentConversation()
      ?? await conversationService.setCurrentConversation(null);

    // Check if this is the first message in a new conversation
    const shouldGenerateTitle = isFirstMessageValue;
    console.log('Should generate title?', shouldGenerateTitle, 'isFirstMessage:', isFirstMessageValue);

    // Set isFirstMessage to false after the first message
    if (isFirstMessageValue) {
      isFirstMessage.set(false);
    }

    // Generate assistant message ID before streaming
    const assistantMessageId = generateMessageId();
    const userMessageId = generateMessageId();
    cancelledAssistantMessageIds.delete(assistantMessageId);
    pendingAssistantMessageId = assistantMessageId;
    startRequestWatchdog(assistantMessageId);

    // Check if Hono server backend is enabled
    if (honoBackend.enabled) {
      const sessionId = honoBackend.getSessionId(currentConversation.id);

      // Add user message directly (Hono doesn't emit message.saved for it)
      messages.update((msgs) => [
        ...msgs,
        {
          id: userMessageId,
          type: 'sent' as const,
          content: currentMessageValue,
          attachments: attachmentsValue.length ? attachmentsValue : undefined,
          timestamp: Date.now(),
        },
      ]);

      honoStreamController = new AbortController();
      try {
        const result = await streamMessageViaHono(
          currentMessageValue,
          {
            conversationId: currentConversation.id,
            messageId: assistantMessageId,
            sessionId,
            model: selectedModelObject && selectedModelValue
              ? `${selectedModelObject.provider}:${selectedModelValue}`
              : selectedModelValue || undefined,
            agent: 'planner',
            systemPrompt: systemPromptContent,
            // Persist session ID eagerly — if the stream is aborted before `done`,
            // the next follow-up message still has the correct session to resume.
            onSessionId: (sid) => honoBackend.setSessionId(currentConversation.id, sid),
          },
          handleAgentEvent,
          honoStreamController.signal,
        );
        if (result.sessionId) {
          honoBackend.setSessionId(currentConversation.id, result.sessionId);
        }
        if (result.agentId) {
          honoAgentId = result.agentId;
        }
      } finally {
        honoStreamController = null;
      }
    // Check if this is a custom backend — call it directly instead of via Tauri
    } else if (selectedModelObject?.provider === 'custom' && selectedModelObject?.custom_backend_id) {
      const customBe = await invoke<CustomBackend | null>('get_custom_backend', {
        id: selectedModelObject.custom_backend_id,
      });
      if (!customBe) {
        throw new Error(`Custom backend not found: ${selectedModelObject.custom_backend_id}`);
      }

      await sendMessageViaCustomBackend(
        currentMessageValue,
        systemPromptContent,
        customBe,
        selectedModelValue,
        assistantMessageId,
        userMessageId,
      );
    } else {
      await invoke('agent_send_message', {
        payload: {
          conversation_id: currentConversation?.id,
          model: selectedModelValue,
          provider: selectedModelObject?.provider || 'openai',
          system_prompt: systemPromptContent,
          content: currentMessageValue,
          attachments: attachmentsValue,
          user_message_id: userMessageId,
          assistant_message_id: assistantMessageId,
          custom_backend_id: selectedModelObject?.custom_backend_id || null,
          stream: streamingEnabledValue,
        }
      });
    }

    // Generate a title for the conversation if this is the first message
    console.log('Generating title for conversation:', currentConversation?.id);
    if (shouldGenerateTitle) {
      console.log('Initiating title generation for conversation:', currentConversation?.id);
      // Use setTimeout to avoid blocking the UI
      setTimeout(async () => {
        try {
          await titleGeneratorService.generateAndUpdateTitle(currentConversation?.id || '');
        } catch (error) {
          console.error('Error generating conversation title:', error);
        }
      }, 1000);
    }
  } catch (error) {
    console.error('Error sending message:', error);
    finalizeRunningToolCalls('Request failed', Date.now());
    resetStreamingState();
  } finally {
    // Loading state cleared on stream completion.
  }
}

export async function cancelCurrentAgentRequest() {
  const messageId = streamingAssistantMessageId || pendingAssistantMessageId;
  if (!messageId) {
    isLoading.set(false);
    return;
  }

  cancelledAssistantMessageIds.add(messageId);

  if (honoStreamController) {
    honoStreamController.abort();
    honoStreamController = null;
  }

  if (honoAgentId) {
    const agentId = honoAgentId;
    try {
      await honoBackend.getClient().cancelAgent(agentId);
    } catch {
      // best-effort
    }
  }

  resetStreamingState();
}

export function clearConversation() {
  // Clear messages immediately
  messages.set([]);
  // Reset first message flag
  isFirstMessage.set(true);
  // Clear streaming state
  resetStreamingState();
  cancelledAssistantMessageIds.clear();
  pendingToolApprovals.set([]);
  toolActivity.set([]);
  toolCallsByMessageId.clear();
  agentPhase.set(null);
  agentPlan.set(null);
  agentPlanSteps.set([]);
  conversationService.setCurrentConversation(null);
  // Reset branch store
  branchStore.reset();
}

export async function resolveToolApproval(
  approvalId: string,
  approved: boolean,
  scope?: ToolExecutionApprovalScope
) {
  if (honoBackend.enabled) {
    const approvals = get(pendingToolApprovals);
    const approval = approvals.find((a) => a.approval_id === approvalId);
    const agentId = approval?.agent_id;
    if (agentId) {
      // Optimistically clear the approval and show loading
      pendingToolApprovals.update((a) => a.filter((x) => x.approval_id !== approvalId));
      isLoading.set(true);

      // Subscribe to the event stream BEFORE sending the approval so we receive
      // events emitted during the resumed agent run (tool activity, text deltas).
      const eventAbort = new AbortController();
      const client = honoBackend.getClient();
      const eventPromise = (async () => {
        let streamStarted = false;
        try {
          for await (const sseEvent of client.subscribeToEvents(agentId, eventAbort.signal)) {
            // Convert SSE event to the same format honoEventBridge uses
            const { event, data } = sseEvent;
            const ts = Date.now();
            const convId = approval?.conversation_id ?? '';
            const msgId = approval?.message_id ?? '';

            if (event === 'tool:started') {
              handleAgentEvent({
                event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED,
                payload: {
                  execution_id: data.callId as string,
                  tool_name: data.name as string,
                  args: (data.args as Record<string, unknown>) ?? {},
                  message_id: msgId,
                  conversation_id: convId,
                  session_id: data.agentId as string | undefined,
                  parent_session_id: (data.parentId as string) ?? null,
                  is_sub_agent: data.parentId != null,
                  timestamp_ms: ts,
                },
                timestamp_ms: ts,
              });
            } else if (event === 'tool:completed') {
              handleAgentEvent({
                event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_COMPLETED,
                payload: {
                  execution_id: data.callId as string,
                  tool_name: (data.name as string) ?? '',
                  result: data.output,
                  success: data.success !== false,
                  duration_ms: data.durationMs as number | undefined,
                  message_id: msgId,
                  conversation_id: convId,
                  session_id: data.agentId as string | undefined,
                  parent_session_id: (data.parentId as string) ?? null,
                  is_sub_agent: data.parentId != null,
                  timestamp_ms: ts,
                },
                timestamp_ms: ts,
              });
            } else if (event === 'tool:proposed') {
              // Emit TOOL_EXECUTION_STARTED first so a tool bubble (segment +
              // ToolCallRecord) is created — mirrors the main bridge's `approval`
              // handler which emits both STARTED and PROPOSED.
              handleAgentEvent({
                event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED,
                payload: {
                  execution_id: data.callId as string,
                  tool_name: data.name as string,
                  args: (data.args as Record<string, unknown>) ?? {},
                  message_id: msgId,
                  conversation_id: convId,
                  session_id: data.agentId as string | undefined,
                  parent_session_id: (data.parentId as string) ?? null,
                  is_sub_agent: data.parentId != null,
                  timestamp_ms: ts,
                },
                timestamp_ms: ts,
              });
              handleAgentEvent({
                event_type: AGENT_EVENT_TYPES.TOOL_EXECUTION_PROPOSED,
                payload: {
                  execution_id: data.callId as string,
                  approval_id: data.callId as string,
                  tool_name: data.name as string,
                  args: (data.args as Record<string, unknown>) ?? {},
                  agent_id: data.agentId as string | undefined,
                  message_id: msgId,
                  conversation_id: convId,
                  iteration: 0,
                  timestamp_ms: ts,
                },
                timestamp_ms: ts,
              });
            } else if (event === 'text:delta') {
              // Only emit root-agent text as assistant stream (sub-agent text goes to delegate tool preview)
              const parentId = data.parentId as string | null | undefined;
              if (!parentId) {
                // Emit STREAM_STARTED on first root-agent text so the streaming
                // handlers (streamingAssistantMessageId gate) accept the chunks.
                if (!streamStarted) {
                  streamStarted = true;
                  handleAgentEvent({
                    event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_STARTED,
                    payload: { conversation_id: convId, message_id: msgId, timestamp_ms: ts },
                    timestamp_ms: ts,
                  });
                }
                handleAgentEvent({
                  event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_CHUNK,
                  payload: { conversation_id: convId, message_id: msgId, chunk: data.text as string, timestamp_ms: ts },
                  timestamp_ms: ts,
                });
              }
            } else if (event === 'tool:denied') {
              const deniedCallId = data.callId as string;
              const deniedName = (data.name as string) ?? '';
              if (msgId) {
                upsertToolCall(msgId, deniedCallId, {
                  tool_name: deniedName,
                  success: false,
                  error: 'Tool execution denied by user',
                  completed_at: ts,
                });
              }
              // Also remove from pending approvals in case the batch-cancel path
              // denied sibling tools that are still shown in the UI.
              pendingToolApprovals.update((a) =>
                a.filter((x) => x.approval_id !== deniedCallId)
              );
            } else if (event === 'agent:completed' || event === 'agent:failed') {
              // Only break when the ROOT agent is done (parentId is null/undefined)
              const parentId = data.parentId as string | null | undefined;
              if (!parentId) break;
            }
          }
        } catch {
          // Event stream closed or aborted — expected
        }
        return streamStarted;
      })();

      try {
        const result = await client.approveToolExecution(
          agentId,
          approvalId,
          approved ? 'approved' : 'denied',
          scope,
        );

        // Stop event subscription — the HTTP response has the final state
        eventAbort.abort();
        const didStream = await eventPromise.catch(() => false);

        // Reconcile tool calls whose events were missed due to the SSE
        // subscription racing against the approval POST.  This covers the
        // approved tool itself, any tools the agent executed in the continued
        // run, and delegate tools whose tool:completed may not have arrived
        // before the event loop broke.
        if (result.status === 'completed' || result.status === 'failed') {
          const msgId = approval?.message_id;
          if (msgId) {
            const entries = toolCallsByMessageId.get(msgId);
            if (entries) {
              for (const [execId, tc] of entries) {
                if (tc.success === undefined) {
                  upsertToolCall(msgId, execId, {
                    success: result.status === 'completed',
                    result: tc.tool_name === 'delegate'
                      ? (result.output?.[result.output.length - 1]?.content ?? '')
                      : undefined,
                    completed_at: Date.now(),
                  });
                }
              }
            }
          }
        }

        // Show completed output if the agent finished.
        // If the event stream already streamed text, finalize via STREAM_COMPLETED
        // so the message is properly committed. Otherwise, fall back to the static
        // output from the HTTP response.
        if (didStream) {
          const convId = approval?.conversation_id ?? '';
          const msgId = approval?.message_id ?? '';
          const lastContent = result.output?.[result.output.length - 1]?.content ?? '';
          handleAgentEvent({
            event_type: AGENT_EVENT_TYPES.ASSISTANT_STREAM_COMPLETED,
            payload: {
              conversation_id: convId,
              message_id: msgId,
              content: typeof lastContent === 'string' ? lastContent : '',
              timestamp_ms: Date.now(),
            },
            timestamp_ms: Date.now(),
          });
        } else if (result.output && result.output.length > 0) {
          const lastOutput = result.output[result.output.length - 1];
          const content = lastOutput.content;
          if (content && typeof content === 'string' && content.trim()) {
            const msgId = approval?.message_id ?? generateMessageId();
            messages.update((msgs) => {
              const idx = msgs.findIndex((m) => m.id === msgId);
              const newMsg: Message = { id: msgId, type: 'received', content, timestamp: Date.now() };
              if (idx === -1) return [...msgs, newMsg];
              return [...msgs.slice(0, idx), { ...msgs[idx], ...newMsg }, ...msgs.slice(idx + 1)];
            });
          }
        }

        // Surface any further pending approvals from the continued run
        if (result.status === 'waiting' && result.waitingFor) {
          for (const w of result.waitingFor) {
            if (w.type === 'approval') {
              const wMsgId = approval?.message_id;
              // Ensure a tool bubble exists for this approval (segment + ToolCallRecord)
              if (wMsgId) {
                ensureAssistantMessageForToolExecution(wMsgId, Date.now());
                upsertToolCall(wMsgId, w.callId, {
                  tool_name: w.name,
                  args: w.args ?? {},
                  started_at: Date.now(),
                });
                appendSegment(wMsgId, { kind: 'tool', execution_id: w.callId });
              }
              pendingToolApprovals.update((a) => {
                if (a.some((x) => x.approval_id === w.callId)) return a;
                return [...a, {
                  execution_id: w.callId,
                  approval_id: w.callId,
                  tool_name: w.name,
                  args: w.args ?? {},
                  agent_id: result.id ?? agentId,
                  message_id: approval?.message_id,
                  conversation_id: approval?.conversation_id,
                  iteration: 0,
                  timestamp_ms: Date.now(),
                }];
              });
            }
          }
        }
      } catch (error) {
        eventAbort.abort();
        console.error('Failed to resolve Hono tool approval:', error);
      } finally {
        isLoading.set(false);
      }
      return;
    }
  }

  try {
    await backend.resolveToolExecutionApproval(approvalId, approved, scope);
  } catch (error) {
    console.error('Failed to resolve tool approval:', error);
  }
}

