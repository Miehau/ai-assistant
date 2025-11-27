import { writable, derived, get } from 'svelte/store';
import type { Message } from '$lib/types';
import type { Model } from '$lib/types/models';
import type { SystemPrompt } from '$lib/types';
import { invoke } from '@tauri-apps/api/tauri';
import { chatService } from '$lib/services/chat';
import { conversationService } from '$lib/services/conversation';
import { titleGeneratorService } from '$lib/services/titleGenerator';
import { modelService, apiKeyService } from '$lib/models';
import { v4 as uuidv4 } from 'uuid';
import { branchStore } from '$lib/stores/branches';

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

// Streaming-specific stores for smooth updates without array reactivity
export const streamingMessage = writable<string>('');
export const isStreaming = writable<boolean>(false);

// Derived stores
export const hasAttachments = derived(
  attachments,
  $attachments => $attachments.length > 0
);

// Actions
export async function loadModels() {
  try {
    console.log('[ChatStore] Starting loadModels...');

    // First load API keys to ensure model availability is updated
    console.log('[ChatStore] Loading API keys...');
    await apiKeyService.loadAllApiKeys();
    console.log('[ChatStore] API keys loaded');

    // Get models from both sources
    console.log('[ChatStore] Loading stored models...');
    const storedModels = await modelService.loadModels();
    console.log('[ChatStore] Stored models count:', storedModels.length);

    console.log('[ChatStore] Getting registry models with capabilities...');
    const registryModels = modelService.getAvailableModelsWithCapabilities();
    console.log('[ChatStore] Registry models count:', registryModels.length);

    // Combine models, prioritizing registry models for their capabilities
    const combinedModels = [...storedModels];

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

    console.log('[ChatStore] Combined models count:', combinedModels.length);

    // Filter to only enabled models
    const enabledModels = combinedModels.filter(model => model.enabled);
    console.log('[ChatStore] Enabled models count:', enabledModels.length);
    console.log('[ChatStore] Enabled models:', enabledModels.map(m => `${m.model_name} (${m.provider})`));

    availableModels.set(enabledModels);

    if (enabledModels.length > 0) {
      selectedModel.set(enabledModels[0].model_name);
      console.log('[ChatStore] Selected default model:', enabledModels[0].model_name);
    } else {
      console.warn('[ChatStore] No enabled models available!');
    }
  } catch (error) {
    console.error('[ChatStore] Failed to load models:', error);
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

  if (!currentMessageValue.trim() && attachmentsValue.length === 0) return;

  isLoading.set(true);

  try {
    // Find the model object to get its display name
    const models = get(availableModels);
    const selectedModelObject = models.find(m => m.model_name === selectedModelValue);

    // Create and display user message immediately with unique ID
    const userMessage: Message = {
      id: generateMessageId(),
      type: 'sent',
      content: currentMessageValue,
      attachments: attachmentsValue.length > 0 ? attachmentsValue : undefined,
      model: selectedModelObject ? `${selectedModelObject.model_name} • ${selectedModelObject.provider}` : selectedModelValue,
    };

    messages.update(msgs => [...msgs, userMessage]);

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

    // Get the current conversation
    const currentConversation = conversationService.getCurrentConversation();

    // Check if this is the first message in a new conversation
    const shouldGenerateTitle = isFirstMessageValue;
    console.log('Should generate title?', shouldGenerateTitle, 'isFirstMessage:', isFirstMessageValue);

    // Set isFirstMessage to false after the first message
    if (isFirstMessageValue) {
      isFirstMessage.set(false);
    }

    // Initialize streaming state - no array updates during streaming!
    isStreaming.set(true);
    streamingMessage.set('');

    // Generate assistant message ID before streaming
    const assistantMessageId = generateMessageId();

    // Use direct chat mode
    const result = await chatService.handleSendMessage(
      currentMessageValue,
      selectedModelValue,
      (chunk: string) => {
        // Update only the streaming store - no array reactivity!
        streamingMessage.update(content => content + chunk);
      },
      systemPromptContent,
      attachmentsValue,
      userMessage.id,  // Pass user message ID
      assistantMessageId  // Pass assistant message ID
    );

    // Streaming complete - add final message to array (single update)
    const finalContent = get(streamingMessage);
    if (finalContent) {
      messages.update(msgs => [...msgs, {
        id: assistantMessageId,  // Use the same ID
        type: 'received',
        content: finalContent
      }]);
    }

    // Clean up streaming state
    isStreaming.set(false);
    streamingMessage.set('');

    // Generate a title for the conversation if this is the first message
    console.log('Generating title for conversation:', currentConversation?.id);
    if (shouldGenerateTitle) {
      console.log('Initiating title generation for conversation:', result?.conversationId);
      // Use setTimeout to avoid blocking the UI
      setTimeout(async () => {
        try {
          await titleGeneratorService.generateAndUpdateTitle(result?.conversationId || '');
        } catch (error) {
          console.error('Error generating conversation title:', error);
        }
      }, 1000);
    }
  } catch (error) {
    console.error('Error sending message:', error);
  } finally {
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
  conversationService.setCurrentConversation(null);
  // Reset branch context
  chatService.resetBranchContext();
  // Reset branch store
  branchStore.reset();
}

// Initialize streaming setting
chatService.setStreamResponse(true);
