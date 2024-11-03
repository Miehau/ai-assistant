import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from './openai';
import type { Message, Model, Attachment } from '$lib/types';
import { conversationService } from './conversation';

export class ChatService {
  private streamResponse = true;  // Default to true
  
  private async getApiKeyForProvider(provider: string): Promise<string> {
    const apiKey = await invoke<string | null>('get_api_key', { provider });
    if (!apiKey) {
      throw new Error(`No API key found for provider: ${provider}`);
    }
    return apiKey;
  }

  private async getModelInfo(modelName: string): Promise<Model> {
    const models = await invoke<Model[]>('get_models');
    const selectedModel = models.find(m => m.model_name === modelName);
    
    if (!selectedModel) {
      throw new Error(`Model ${modelName} not found`);
    }
    
    return selectedModel;
  }

  createMessage(content: string, attachments?: Attachment[]): Message {
    return {
      type: "sent",
      content: content.trim(),
      attachments: attachments?.length ? attachments : undefined
    };
  }

  // this will come from config
  setStreamResponse(value: boolean) {
    this.streamResponse = value;
  }

  async handleSendMessage(
    content: string,
    model: string,
    onStreamResponse: (chunk: string) => void,
    systemPrompt?: string,
    attachments: Attachment[] = [],
  ) {
    try {
      const message = this.createMessage(content, attachments);
      
      const conversation = conversationService.getCurrentConversation() 
        ?? await conversationService.setCurrentConversation(null);
      
      const history = await conversationService.getAPIHistory(conversation.id);
      
      const selectedModel = await this.getModelInfo(model);
      const apiKey = await this.getApiKeyForProvider(selectedModel.provider);
      
      const openAIService = new OpenAIService(apiKey);
      const modelResponse = await openAIService.createChatCompletion(
        model,
        history,
        message,
        systemPrompt,
        this.streamResponse,
        onStreamResponse
      );

      await Promise.all([
        conversationService.saveMessage('user', message.content, message.attachments || []),
        conversationService.saveMessage('assistant', modelResponse, [])
      ]);

      return {
        text: modelResponse,
        conversationId: conversation.id,
      };
    } catch (error) {
      console.error('Failed to send chat message:', error);
      throw error;
    }
  }
}

export const chatService = new ChatService();
