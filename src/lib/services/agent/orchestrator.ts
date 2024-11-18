import type { Message, Attachment } from '$lib/types';
import { IntentAnalysisService } from './intentAnalysis';
import { chatService } from '../chat';
import { invoke } from '@tauri-apps/api/tauri';

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
      
      // Analyze the intent first
      const intent = await this.intentAnalysis.analyzeIntent(content);
      
      if (intent.type === 'memorise') {
        // Handle memorisation intent
        const response = await this.intentAnalysis.handleIntent(intent, content);
        return {
          text: response,
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
}

export const orchestratorService = new OrchestratorService(); 