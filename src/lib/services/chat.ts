import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from './openai';
import { customProviderService } from './customProvider';
import type { Message, Attachment } from '$lib/types';
import { conversationService } from './conversation';
import type { Model } from '$lib/types/models';
import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';
import { formatMessages } from './messageFormatting';
import { AnthropicService } from './anthropic';

export class ChatService {
  private streamResponse = true;
  private currentController: AbortController | null = null;

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

  setStreamResponse(value: boolean) {
    this.streamResponse = value;
  }

  cancelCurrentRequest() {
    if (this.currentController) {
      this.currentController.abort();
      this.currentController = null;
    }
  }

  async handleSendMessage(
    content: string,
    model: string,
    onStreamResponse: (chunk: string) => void,
    systemPrompt?: string,
    attachments: Attachment[] = [],
  ) {
    try {
      this.currentController = new AbortController();
      const message = this.createMessage(content, attachments);
      
      const conversation = conversationService.getCurrentConversation() 
        ?? await conversationService.setCurrentConversation(null);
      
      const history = await conversationService.getAPIHistory(conversation.id);
      const selectedModel = await this.getModelInfo(model);
      
      const modelResponse = await this.createChatCompletion(
        selectedModel,
        history,
        message,
        systemPrompt || "You are a helpful AI assistant.",
        this.streamResponse,
        onStreamResponse,
        this.currentController.signal
      );

      this.currentController = null;

      await Promise.all([
        conversationService.saveMessage('user', message.content, message.attachments || []),
        conversationService.saveMessage('assistant', modelResponse, [])
      ]);

      return {
        text: modelResponse,
        conversationId: conversation.id,
      };
    } catch (error: unknown) {
      if (error instanceof Error && error.name === 'AbortError') {
        console.log('Request was cancelled');
        return;
      }
      console.error('Failed to send chat message:', error);
      throw error;
    }
  }

  private async formatMessages(
    history: any[],
    message: Message,
    systemPrompt: string
  ): Promise<ChatCompletionMessageParam[]> {
    const messages = [
      { role: 'system', content: systemPrompt },
      ...history.map(msg => ({
        role: msg.role,
        content: msg.content,
      })),
      {
        role: 'user',
        content: message.content,
      },
    ] as ChatCompletionMessageParam[];

    return messages;
  }

  private async createChatCompletion(
    model: Model, 
    history: any[], 
    message: Message, 
    systemPrompt: string,
    streamResponse: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal
  ): Promise<string> {
    
    const formattedMessages = await formatMessages(history, message, systemPrompt);

    if (model.provider === 'openai') {
      const apiKey = await this.getApiKeyForProvider(model.provider);
      const openAIService = new OpenAIService(apiKey);
      return openAIService.createChatCompletion(
        model.model_name,
        formattedMessages,
        streamResponse,
        onStreamResponse,
        signal
      );
    } 
    
    if (model.provider === 'anthropic') {
      const apiKey = await this.getApiKeyForProvider(model.provider);
      const anthropicService = new AnthropicService(apiKey);
      return anthropicService.createChatCompletion(
        model.model_name,
        formattedMessages,
        streamResponse,
        onStreamResponse,
        signal
      );
    }
    
    if (model.provider === 'custom' && model.url) {
      return customProviderService.createChatCompletion(
        model.model_name,
        model.url,
        formattedMessages,
        streamResponse,
        onStreamResponse,
        signal
      );
    }
    
    throw new Error(`Unsupported provider: ${model.provider}`);
  }
}

export const chatService = new ChatService();
