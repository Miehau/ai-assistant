import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from './openai';
import type { Message } from '$lib/types';

async function getApiKeyForProvider(provider: string): Promise<string> {
  const apiKey = await invoke<string | null>('get_api_key', { provider });
  if (!apiKey) {
    throw new Error(`No API key found for provider: ${provider}`);
  }
  return apiKey;
}

// Add new interface for attachment
interface Attachment {
  type: string;
  base64Data: string;
}

export async function sendChatMessage(
  message: Message,
  conversationId: string | null,
  model: string,
  streamResponse: boolean,
  onStream?: (chunk: string) => void,
  systemPrompt?: string,
  attachment?: Attachment
) {
  try {
    const conversation = await invoke('get_or_create_conversation', { conversationId });
    const history = await invoke('get_conversation_history', { conversationId: conversation.id });
    
    const models = await invoke<Array<{ model_name: string, provider: string }>>('get_models');
    const selectedModel = models.find(m => m.model_name === model);
    
    if (!selectedModel) {
      throw new Error(`Model ${model} not found`);
    }

    const apiKey = await getApiKeyForProvider(selectedModel.provider);
    const openAIService = new OpenAIService(apiKey);

    const fullResponse = await openAIService.createChatCompletion(
      model,
      history,
      message,
      systemPrompt,
      streamResponse,
      onStream
    );

    // Save messages
    await invoke('save_message', { 
      conversationId: conversation.id, 
      role: 'user', 
      content: message.content,
      attachments: message.attachments || []
    });
    await invoke('save_message', { 
      conversationId: conversation.id, 
      role: 'assistant', 
      content: fullResponse,
      attachments: []
    });

    return {
      text: fullResponse,
      conversationId: conversation.id,
    };
  } catch (error) {
    console.error('Failed to send chat message:', error);
    throw error;
  }
}

export async function getConversationHistory(conversationId: string) {
  try {
    const history: { role: string, content: string }[] = await invoke('get_conversation_history', { conversationId });
    return history;
  } catch (error) {
    console.error('Failed to get conversation history:', error);
    throw new Error('Failed to get conversation history');
  }
}

export async function getConversations() {
  try {
    const conversations: { id: string, name: string }[] = await invoke('get_conversations');
    return conversations;
  } catch (error) {
    console.error('Failed to get conversations:', error);
    throw new Error('Failed to get conversations');
  }
}
