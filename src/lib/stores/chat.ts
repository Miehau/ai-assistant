import { writable, derived, get } from 'svelte/store';
import type { Message } from '$lib/types';
import type { Model } from '$lib/types/models';
import type { SystemPrompt } from '$lib/types';
import { invoke } from '@tauri-apps/api/tauri';
import { chatService } from '$lib/services/chat';
import { conversationService } from '$lib/services/conversation';
import { titleGeneratorService } from '$lib/services/titleGenerator';
import { modelService, apiKeyService } from '$lib/models';
import { customBackendService } from '$lib/services/customBackendService.svelte';
import { ollamaService } from '$lib/services/ollamaService.svelte';
import { backend } from '$lib/backend/client';
import { v4 as uuidv4 } from 'uuid';
import { branchStore } from '$lib/stores/branches';
import { startAgentEventBridge } from '$lib/services/eventBridge';
import { AGENT_EVENT_TYPES } from '$lib/types/events';
import type { AgentEvent, Attachment, ToolCallRecord } from '$lib/types';
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

// Streaming-specific stores for smooth updates without array reactivity
export const streamingMessage = writable<string>('');
export const isStreaming = writable<boolean>(false);

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
let streamingAssistantMessageId: string | null = null;
let streamingChunkBuffer = '';
let streamingFlushPending = false;
let pendingAssistantMessageId: string | null = null;
const TOOL_ACTIVITY_LIMIT = 8;
const toolCallsByMessageId = new Map<string, Map<string, ToolCallRecord>>();

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
  const next: ToolCallRecord = {
    execution_id: executionId,
    tool_name: payload.tool_name ?? existing?.tool_name ?? 'unknown',
    args: payload.args ?? existing?.args ?? {},
    result: payload.result ?? existing?.result,
    success: payload.success ?? existing?.success,
    error: payload.error ?? existing?.error,
    duration_ms: payload.duration_ms ?? existing?.duration_ms,
    started_at: payload.started_at ?? existing?.started_at,
    completed_at: payload.completed_at ?? existing?.completed_at,
  };

  entries.set(executionId, next);
  messages.update((msgs) =>
    msgs.map((msg) =>
      msg.id === messageId ? { ...msg, tool_calls: getToolCallsForMessage(messageId) } : msg
    )
  );
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
  const status: ToolActivityEntry['status'] = execution.success ? 'completed' : 'failed';
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

  stopAgentEventBridge = await startAgentEventBridge((event: AgentEvent) => {
    if (event.event_type === AGENT_EVENT_TYPES.MESSAGE_SAVED) {
      const payload = event.payload as MessageSavedPayload;
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      const isAssistant = payload.role !== 'user';
      if (isAssistant && payload.tool_executions && payload.tool_executions.length > 0) {
        for (const execution of payload.tool_executions) {
          upsertToolCall(payload.message_id, execution.id, {
            tool_name: execution.tool_name,
            args: execution.parameters ?? {},
            result: execution.result,
            success: execution.success,
            error: execution.error ?? undefined,
            duration_ms: execution.duration_ms,
            completed_at: execution.timestamp_ms,
          });

          upsertToolActivityFromExecution({
            execution_id: execution.id,
            tool_name: execution.tool_name,
            success: execution.success,
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
      messages.set([]);
      isFirstMessage.set(true);
      isStreaming.set(false);
      streamingMessage.set('');
      streamingAssistantMessageId = null;
      pendingAssistantMessageId = null;
      isLoading.set(false);
      pendingToolApprovals.set([]);
      toolActivity.set([]);
      toolCallsByMessageId.clear();
      agentPhase.set(null);
      agentPlan.set(null);
      agentPlanSteps.set([]);
    }

    if (event.event_type === AGENT_EVENT_TYPES.ASSISTANT_STREAM_STARTED) {
      const payload = event.payload as AssistantStreamStartedPayload;
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
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      if (streamingAssistantMessageId !== payload.message_id) {
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
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== payload.conversation_id) {
        return;
      }

      if (streamingAssistantMessageId !== payload.message_id) {
        return;
      }

      const toolCalls = getToolCallsForMessage(payload.message_id);
      const hasToolCalls = Boolean(toolCalls && toolCalls.length > 0);
      const hasContent = payload.content.trim().length > 0;

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
          };
          const next = [...msgs];
          next[existingIndex] = updated;
          return next;
        });
      }

      streamingAssistantMessageId = null;
      pendingAssistantMessageId = null;
      isStreaming.set(false);
      streamingMessage.set('');
      streamingChunkBuffer = '';
      streamingFlushPending = false;
      isLoading.set(false);
    }

    if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_COMPLETED) {
      const payload = event.payload as ToolExecutionCompletedPayload;
      const currentConversation = conversationService.getCurrentConversation();
      if (payload.conversation_id && currentConversation?.id !== payload.conversation_id) {
        return;
      }

      if (payload.message_id) {
        ensureAssistantMessageForToolExecution(payload.message_id, payload.timestamp_ms);
        upsertToolCall(payload.message_id, payload.execution_id, {
          tool_name: payload.tool_name,
          result: payload.result,
          success: payload.success,
          error: payload.error,
          duration_ms: payload.duration_ms,
          completed_at: payload.timestamp_ms,
        });
      }

      toolActivity.update((entries) => {
        const status: ToolActivityEntry['status'] = payload.success ? 'completed' : 'failed';
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
    }

    if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_PROPOSED) {
      const payload = event.payload as ToolExecutionProposedPayload;
      const currentConversation = conversationService.getCurrentConversation();
      if (payload.conversation_id && currentConversation?.id !== payload.conversation_id) {
        return;
      }

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
    }

    if (event.event_type === AGENT_EVENT_TYPES.AGENT_PHASE_CHANGED) {
      const payload = event.payload as AgentPhaseChangedPayload;
      agentPhase.set(payload.phase as PhaseKind);
      if (isNeedsHumanInputPhase(payload.phase)) {
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
  });
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

export async function loadConversationHistory(conversationId: string) {
  try {
    const loadedMessages = await conversationService.getDisplayHistory(conversationId);
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
  streamingEnabled.update(value => {
    const newValue = !value;
    chatService.setStreamResponse(newValue);
    return newValue;
  });
}

// Helper to generate unique message IDs using UUID v4
function generateMessageId(): string {
  return uuidv4();
}

export async function sendMessage() {
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
    pendingAssistantMessageId = assistantMessageId;

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
    pendingAssistantMessageId = null;
    isLoading.set(false);
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

  try {
    await invoke('agent_cancel', { message_id: messageId });
  } catch (error) {
    console.error('Failed to cancel agent request:', error);
  } finally {
    streamingAssistantMessageId = null;
    pendingAssistantMessageId = null;
    isStreaming.set(false);
    streamingMessage.set('');
    streamingChunkBuffer = '';
    streamingFlushPending = false;
    isLoading.set(false);
  }
}

export function clearConversation() {
  // Clear messages immediately
  messages.set([]);
  // Reset first message flag
  isFirstMessage.set(true);
  // Clear streaming state
  isStreaming.set(false);
  streamingMessage.set('');
  streamingAssistantMessageId = null;
  pendingAssistantMessageId = null;
  pendingToolApprovals.set([]);
  toolActivity.set([]);
  toolCallsByMessageId.clear();
  agentPhase.set(null);
  agentPlan.set(null);
  agentPlanSteps.set([]);
  conversationService.setCurrentConversation(null);
  // Reset branch context
  chatService.resetBranchContext();
  // Reset branch store
  branchStore.reset();
}

export async function resolveToolApproval(
  approvalId: string,
  approved: boolean,
  scope?: ToolExecutionApprovalScope
) {
  try {
    await backend.resolveToolExecutionApproval(approvalId, approved, scope);
  } catch (error) {
    console.error('Failed to resolve tool approval:', error);
  }
}

// Initialize streaming setting
chatService.setStreamResponse(true);
