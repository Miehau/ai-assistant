import type { Message, Attachment } from '$lib/types';
import { IntentAnalysisService } from './intentAnalysis';
import { chatService } from '../chat';
import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from '../openai';
import type { Model } from '$lib/types/models';
import { message } from 'sveltekit-superforms';
import { conversationService } from '../conversation';

export class OrchestratorService {
  private intentAnalysis: IntentAnalysisService;
  private streamResponse = true;
  private currentController: AbortController | null = null;

  constructor() {
    // Initialize with null, will be set when we get the API key
    this.intentAnalysis = null;
  }

  private async initializeIntentAnalysis() {
    if (!this.intentAnalysis) {
      const apiKey = await invoke<string>('get_api_key', { provider: 'openai' });
      if (!apiKey) {
        throw new Error('OpenAI API key not found');
      }
      this.intentAnalysis = new IntentAnalysisService(apiKey);
    }
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
      await this.initializeIntentAnalysis();
      this.currentController = new AbortController();
      const conversation = conversationService.getCurrentConversation() 
        ?? await conversationService.setCurrentConversation(null);

      // Step 1: Process audio attachments and get transcripts
      const processedAttachments = await this.processAttachments(attachments, content);

      // Step 2: Prepare the content by adding transcripts
      let processedContent = content;
      const audioTranscripts = processedAttachments
        .filter(att => att.attachment_type.startsWith("audio") && att.transcript)
        .map(att => `[Audio Transcript]: ${att.transcript}`);
      
      if (audioTranscripts.length > 0) {
        processedContent += '\n' + audioTranscripts.join('\n');
      }
      
      const intent = await this.intentAnalysis.analyzeIntent(processedContent);
      
      if (intent.type === 'memorise') {
        // Handle memorisation intent
        const response = await this.intentAnalysis.handleIntent(intent, content);
        onStreamResponse(response || '');
        await Promise.all([
          conversationService.saveMessage('user', content, []),
          conversationService.saveMessage('assistant', response || '', [])
        ]);
        return {
          text: response || '',
          conversationId: null, // You might want to handle this differently
        };
      }
      
      // For all other intents, delegate to the regular chat service
      return chatService.handleSendMessage(
        content,
        model,
        onStreamResponse,
        systemPrompt,
        attachments
      );
    } catch (error) {
      console.error('Failed in orchestrator:', error);
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
}

export const orchestratorService = new OrchestratorService(); 