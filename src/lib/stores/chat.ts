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
import { v4 as uuidv4 } from 'uuid';
import { branchStore } from '$lib/stores/branches';
import { startAgentEventBridge } from '$lib/services/eventBridge';
import { AGENT_EVENT_TYPES } from '$lib/types/events';
import type { AgentEvent, Attachment } from '$lib/types';
import type {
  ToolExecutionCompletedPayload,
  ToolExecutionProposedPayload,
  ToolExecutionStartedPayload,
} from '$lib/types/events';
import { currentConversationUsage } from '$lib/stores/tokenUsage';

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
let stopAgentEventBridge: (() => void) | null = null;
let streamingAssistantMessageId: string | null = null;
let streamingChunkBuffer = '';
let streamingFlushPending = false;
const TOOL_ACTIVITY_LIMIT = 8;

export type ToolActivityEntry = {
  execution_id: string;
  tool_name: string;
  status: 'running' | 'completed' | 'failed';
  started_at: number;
  completed_at?: number;
  duration_ms?: number;
  error?: string;
};

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
  stopAgentEventBridge = await startAgentEventBridge((event: AgentEvent) => {
    if (event.event_type === AGENT_EVENT_TYPES.MESSAGE_SAVED) {
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== event.payload.conversation_id) {
        return;
      }

      messages.update((msgs) => {
        if (msgs.some((msg) => msg.id === event.payload.message_id)) {
          return msgs;
        }

        const attachments: Attachment[] = event.payload.attachments.map((attachment) => ({
          name: attachment.name,
          data: attachment.data,
          attachment_type: attachment.attachment_type as Attachment['attachment_type'],
          description: attachment.description,
          transcript: attachment.transcript,
        }));

        const newMessage: Message = {
          id: event.payload.message_id,
          type: event.payload.role === 'user' ? 'sent' : 'received',
          content: event.payload.content,
          attachments: attachments.length ? attachments : undefined,
          timestamp: event.payload.timestamp_ms,
        };

        return [...msgs, newMessage];
      });
    }

    if (event.event_type === AGENT_EVENT_TYPES.USAGE_UPDATED) {
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== event.payload.conversation_id) {
        return;
      }

      currentConversationUsage.set({
        conversation_id: event.payload.conversation_id,
        total_prompt_tokens: event.payload.total_prompt_tokens,
        total_completion_tokens: event.payload.total_completion_tokens,
        total_tokens: event.payload.total_tokens,
        total_cost: event.payload.total_cost,
        message_count: event.payload.message_count,
        last_updated: new Date(event.payload.timestamp_ms).toISOString(),
      });
    }

    if (event.event_type === AGENT_EVENT_TYPES.CONVERSATION_UPDATED) {
      conversationService.applyConversationUpdate(
        event.payload.conversation_id,
        event.payload.name
      );
    }

    if (event.event_type === AGENT_EVENT_TYPES.CONVERSATION_DELETED) {
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== event.payload.conversation_id) {
        return;
      }

      conversationService.applyConversationDeleted(event.payload.conversation_id);
      messages.set([]);
      isFirstMessage.set(true);
      isStreaming.set(false);
      streamingMessage.set('');
      streamingAssistantMessageId = null;
      isLoading.set(false);
      pendingToolApprovals.set([]);
      toolActivity.set([]);
    }

    if (event.event_type === AGENT_EVENT_TYPES.ASSISTANT_STREAM_STARTED) {
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== event.payload.conversation_id) {
        return;
      }

      streamingAssistantMessageId = event.payload.message_id;
      isStreaming.set(true);
      streamingMessage.set('');
      isLoading.set(true);
    }

    if (event.event_type === AGENT_EVENT_TYPES.ASSISTANT_STREAM_CHUNK) {
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== event.payload.conversation_id) {
        return;
      }

      if (streamingAssistantMessageId !== event.payload.message_id) {
        return;
      }

      streamingChunkBuffer += event.payload.chunk;

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
      const currentConversation = conversationService.getCurrentConversation();
      if (!currentConversation || currentConversation.id !== event.payload.conversation_id) {
        return;
      }

      if (streamingAssistantMessageId !== event.payload.message_id) {
        return;
      }

      messages.update((msgs) => {
        if (msgs.some((msg) => msg.id === event.payload.message_id)) {
          return msgs;
        }

        const newMessage: Message = {
          id: event.payload.message_id,
          type: 'received',
          content: event.payload.content,
          timestamp: event.payload.timestamp_ms,
        };

        return [...msgs, newMessage];
      });

      streamingAssistantMessageId = null;
      isStreaming.set(false);
      streamingMessage.set('');
      streamingChunkBuffer = '';
      streamingFlushPending = false;
      isLoading.set(false);
    }

    if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_STARTED) {
      const payload = event.payload as ToolExecutionStartedPayload;
      const currentConversation = conversationService.getCurrentConversation();
      if (payload.conversation_id && currentConversation?.id !== payload.conversation_id) {
        return;
      }

      toolActivity.update((entries) => {
        const next = entries.filter((entry) => entry.execution_id !== payload.execution_id);
        next.unshift({
          execution_id: payload.execution_id,
          tool_name: payload.tool_name,
          status: 'running',
          started_at: payload.timestamp_ms,
        });
        return next.slice(0, TOOL_ACTIVITY_LIMIT);
      });
    }

    if (event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_COMPLETED) {
      const payload = event.payload as ToolExecutionCompletedPayload;
      const currentConversation = conversationService.getCurrentConversation();
      if (payload.conversation_id && currentConversation?.id !== payload.conversation_id) {
        return;
      }

      toolActivity.update((entries) => {
        const status = payload.success ? 'completed' : 'failed';
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
      const currentConversation = conversationService.getCurrentConversation();
      if (event.payload.conversation_id && currentConversation?.id !== event.payload.conversation_id) {
        return;
      }

      pendingToolApprovals.update((approvals) => {
        if (approvals.some((entry) => entry.approval_id === event.payload.approval_id)) {
          return approvals;
        }
        return [...approvals, event.payload];
      });
    }

    if (
      event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_APPROVED ||
      event.event_type === AGENT_EVENT_TYPES.TOOL_EXECUTION_DENIED
    ) {
      pendingToolApprovals.update((approvals) =>
        approvals.filter((entry) => entry.approval_id !== event.payload.approval_id)
      );
    }
  });
}

// Actions
export async function loadModels() {
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

    if (enabledModels.length > 0) {
      // Try to restore last used model
      const lastUsedModel = await getLastUsedModel();
      const modelToSelect = lastUsedModel && enabledModels.some(m => m.model_name === lastUsedModel)
        ? lastUsedModel
        : enabledModels[0].model_name;

      selectedModel.set(modelToSelect);
      console.log('[ChatStore] Selected model:', modelToSelect, lastUsedModel ? '(restored from preferences)' : '(default)');
    } else {
      console.warn('[ChatStore] No enabled models available!');
    }
  } catch (error) {
    console.error('[ChatStore] Failed to load models:', error);
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

export async function loadSystemPrompts() {
  try {
    const prompts = await invoke<SystemPrompt[]>('get_all_system_prompts');
    systemPrompts.set(prompts);

    if (prompts.length > 0) {
      selectedSystemPrompt.set(prompts[0]);
    }
  } catch (error) {
    console.error('Failed to load system prompts:', error);
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
    const selectedModelObject = models.find(m => m.model_name === selectedModelValue);

    // Clear input fields
    currentMessage.set('');
    attachments.set([]);

    // Default system prompt
    const defaultSystemPrompt = 'You are a helpful assistant.';

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
    isLoading.set(false);
  } finally {
    // Loading state cleared on stream completion.
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
  pendingToolApprovals.set([]);
  toolActivity.set([]);
  conversationService.setCurrentConversation(null);
  // Reset branch context
  chatService.resetBranchContext();
  // Reset branch store
  branchStore.reset();
}

export async function resolveToolApproval(approvalId: string, approved: boolean) {
  try {
    await invoke('resolve_tool_execution_approval', {
      approval_id: approvalId,
      approved,
    });
  } catch (error) {
    console.error('Failed to resolve tool approval:', error);
  }
}

// Initialize streaming setting
chatService.setStreamResponse(true);
