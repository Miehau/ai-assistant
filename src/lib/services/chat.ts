import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from './openai';
import { customProviderService } from './customProvider';
import type { Message, Attachment } from '$lib/types';
import { conversationService } from './conversation';
import type { Model } from '$lib/types/models';
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
      
      // Step 1: Process audio attachments and get transcripts
      const processedAttachments = await this.processAttachments(attachments, content);

      // Step 2: Prepare the content by adding transcripts
      let processedContent = content;
      const audioTranscripts = processedAttachments
        .filter(att => att.attachment_type.startsWith("audio") && att.transcript)
        .map(att => `[Audio Transcript]: ${att.transcript}`);
      
      if (audioTranscripts.length > 0) {
        // processedContent += '\n' + audioTranscripts.join('\n');
      }

      // Step 3: Create the message with processed content and attachments
      const message = this.createMessage(processedContent, processedAttachments);
      console.log(message);
      
      // Step 4: Get or create conversation and fetch history
      const conversation = conversationService.getCurrentConversation() 
        ?? await conversationService.setCurrentConversation(null);
      const history = await conversationService.getAPIHistory(conversation.id);
      
      // Step 5: Get model info
      const selectedModel = await this.getModelInfo(model);
      
      // Step 6: Send to AI and get response
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

      // Step 7: Save both messages to the conversation
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

  private async processAttachments(attachments: Attachment[], content: string): Promise<Attachment[]> {
    const processedAttachments = [...attachments];
    
    for (const attachment of processedAttachments) {
      if (attachment.attachment_type.startsWith("audio") && !attachment.transcript) {
        try {
          const apiKey = await this.getApiKeyForProvider('openai');
          const openAIService = new OpenAIService(apiKey);
          attachment.transcript = await openAIService.transcribeAudio(attachment.data, content);
        } catch (error) {
          console.error('Failed to transcribe audio:', error);
          attachment.transcript = '[Transcription failed]';
        }
      }
    }
    
    return processedAttachments;
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

  async transcribeAudio(base64Audio: string): Promise<string> {
    const apiKey = await this.getApiKeyForProvider('openai');
    const openAIService = new OpenAIService(apiKey);
    return openAIService.transcribeAudio(base64Audio);
  }
}

export const chatService = new ChatService();
