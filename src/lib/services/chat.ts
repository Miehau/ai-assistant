import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from './openai';
import type { Message } from '$lib/types';
import { conversationService } from './conversation';

async function getApiKeyForProvider(provider: string): Promise<string> {
  const apiKey = await invoke<string | null>('get_api_key', { provider });
  if (!apiKey) {
    throw new Error(`No API key found for provider: ${provider}`);
  }
  return apiKey;
}


export async function sendChatMessage(
  message: Message,
  model: string,
  streamResponse: boolean,
  onStream?: (chunk: string) => void,
  systemPrompt?: string,
) {
  try {
    const currentConversation = conversationService.getCurrentConversation();
    if (!currentConversation) {
      await conversationService.setCurrentConversation(null); // Creates new conversation
    }
    
    const conversation = conversationService.getCurrentConversation()!;
    const history = await conversationService.getHistory(conversation.id);
    
    const models = await invoke<Array<{ model_name: string, provider: string }>>('get_models');
    console.log(model);
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

    // Simpler message saving
    await conversationService.saveMessage(
      'user', 
      message.content,
      message.attachments || []
    );
    
    await conversationService.saveMessage(
      'assistant', 
      fullResponse,
      []
    );

    return {
      text: fullResponse,
      conversationId: conversation.id,
    };
  } catch (error) {
    console.error('Failed to send chat message:', error);
    throw error;
  }
}
