import { writable, derived, get } from 'svelte/store';
import type { Message } from '$lib/types';
import type { Model } from '$lib/types/models';
import type { SystemPrompt } from '$lib/types';
import type { Selected } from 'bits-ui';
import { invoke } from '@tauri-apps/api/tauri';
import { chatService } from '$lib/services/chat';
import { conversationService } from '$lib/services/conversation';
import { titleGeneratorService } from '$lib/services/titleGenerator';
import { modelService, apiKeyService } from '$lib/models';

// State stores
export const messages = writable<Message[]>([]);
export const availableModels = writable<Model[]>([]);
export const systemPrompts = writable<SystemPrompt[]>([]);
export const selectedModel = writable<Selected<string>>({
  value: '',
  label: 'No models'
});
export const selectedSystemPrompt = writable<SystemPrompt | null>(null);
export const streamingEnabled = writable<boolean>(true);
export const isLoading = writable<boolean>(false);
export const attachments = writable<any[]>([]);
export const currentMessage = writable<string>('');
export const isFirstMessage = writable<boolean>(true);

// Derived stores
export const hasAttachments = derived(
  attachments,
  $attachments => $attachments.length > 0
);

// Actions
export async function loadModels() {
  try {
    // First load API keys to ensure model availability is updated
    await apiKeyService.loadAllApiKeys();
    
    // Get models from both sources
    const storedModels = await modelService.loadModels();
    const registryModels = modelService.getAvailableModelsWithCapabilities();
    
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
    
    // Filter to only enabled models
    const enabledModels = combinedModels.filter(model => model.enabled);
    availableModels.set(enabledModels);

    if (enabledModels.length > 0) {
      selectedModel.set({
        value: enabledModels[0].model_name,
        label: `${enabledModels[0].model_name} • ${enabledModels[0].provider}`
      });
    }
  } catch (error) {
    console.error('Failed to load models:', error);
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

export async function sendMessage() {
  let currentMessageValue = '';
  let attachmentsValue: any[] = [];
  let selectedModelValue: Selected<string> = { value: '', label: '' };
  let selectedSystemPromptValue: SystemPrompt | null = null;
  let isFirstMessageValue = false;
  
  // Get current values from stores
  currentMessage.subscribe(value => { currentMessageValue = value; })();
  attachments.subscribe(value => { attachmentsValue = [...value]; })();
  selectedModel.subscribe(value => { selectedModelValue = value; })();
  selectedSystemPrompt.subscribe(value => { selectedSystemPromptValue = value; })();
  isFirstMessage.subscribe(value => { isFirstMessageValue = value; })();
  
  if (!currentMessageValue.trim() && attachmentsValue.length === 0) return;
  
  isLoading.set(true);
  
  try {
    // Create and display user message immediately
    const userMessage: Message = {
      type: 'sent',
      content: currentMessageValue,
      attachments: attachmentsValue.length > 0 ? attachmentsValue : undefined,
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
    
    const result = await chatService.handleSendMessage(
      currentMessageValue,
      selectedModelValue.value,
      (chunk: string) => {
        messages.update(msgs => {
          if (!msgs[msgs.length - 1] || msgs[msgs.length - 1].type !== 'received') {
            return [...msgs, { type: 'received', content: chunk }];
          } else {
            const updatedMsgs = [...msgs];
            updatedMsgs[updatedMsgs.length - 1].content += chunk;
            return updatedMsgs;
          }
        });
      },
      systemPromptContent,
      attachmentsValue,
    );
    
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
  conversationService.setCurrentConversation(null);
}

// Initialize streaming setting
chatService.setStreamResponse(true);
