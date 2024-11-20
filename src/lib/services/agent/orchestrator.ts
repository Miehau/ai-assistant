import type { Message, Attachment } from '$lib/types';
import { IntentAnalysisService } from './intentAnalysis';
import { chatService } from '../chat';
import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from '../openai';
import type { Model } from '$lib/types/models';
import { message } from 'sveltekit-superforms';
import { conversationService } from '../conversation';
import type { Tool } from './tools/types';
import { webFetcher } from './tools/webFetcher';
import { ProcessHtmlTool } from './tools/processHtml';
import { formatMessages } from '../messageFormatting';

export class OrchestratorService {
  private intentAnalysis: IntentAnalysisService;
  private currentController: AbortController | null = null;
  private tools: Record<string, Tool>;

  constructor() {
    // Initialize with null, will be set when we get the API key
    this.intentAnalysis = null;
    this.tools = [webFetcher].reduce((acc, tool) => ({
      ...acc,
      [tool.name]: tool
    }), {} as Record<string, Tool>);
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

  cancelCurrentRequest() {
    if (this.currentController) {
      this.currentController.abort();
      this.currentController = null;
    }
  }

  createMessage(content: string, attachments?: Attachment[]): Message {
    return {
      type: "sent",
      content: content.trim(),
      attachments: attachments?.length ? attachments : undefined
    };
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
        
      let conversationHistory = await conversationService.getAPIHistory(conversation.id);
      console.log('Conversation history:', conversationHistory);
      // Step 1: Process audio attachments and get transcripts
      const processedAttachments = await this.processAttachments(attachments, content);

      // Step 2: Prepare the content by adding transcripts
      let processedContent = content;
      processedContent = appendAudioTranscripts(processedAttachments, processedContent);
      console.log('Processed content:', processedContent);
      let currentIntent = await this.intentAnalysis.analyzeIntent(processedContent, conversationHistory);
      conversationHistory.push({
        role: 'assistant',
        content: "Your inner dialog and intent analysis: " + JSON.stringify(currentIntent)
      });
      let finalResponse = '';
      // this should be replaced with a tool calls in next iteration
      while (currentIntent.intent_type === 'tool_call') {
        let tool = this.tools[currentIntent.tool || ''];
        let toolResult = await tool.execute(currentIntent.params);
        conversationHistory.push({role: 'assistant', content: toolResult.result || ''});
        currentIntent = await this.intentAnalysis.analyzeIntent(toolResult.result || '', conversationHistory);
        console.log('Current intent:', currentIntent);
        finalResponse = toolResult.result || '';
      }

      await Promise.all([
        conversationService.saveMessage('user', content, []),
        conversationService.saveMessage('assistant', finalResponse || '', [])
      ]);
      
      // For all other intents, delegate to the regular chat service
      const apiKey = await this.getApiKeyForProvider('openai');
      const openAIService = new OpenAIService(apiKey);
      const response = await openAIService.createChatCompletion(
        model,
        await formatMessages(conversationHistory, this.createMessage(processedContent, processedAttachments), systemPrompt),
        true,
        onStreamResponse,
        this.currentController.signal
      );
      return response;
    } catch (error) {
      console.error('Failed in orchestrator:', error);
      throw error;
    }

    function appendAudioTranscripts(processedAttachments: Attachment[], processedContent: string) {
      const audioTranscripts = processedAttachments
        .filter(att => att.attachment_type.startsWith("audio") && att.transcript)
        .map(att => `[Audio Transcript]: ${att.transcript}`);

      if (audioTranscripts.length > 0) {
        processedContent += '\n' + audioTranscripts.join('\n');
      }

      const textAttachments = processedAttachments
        .filter(att => att.attachment_type.startsWith("text") && att.data)
        .map(att => `[Text attachment: ${att.data}]`);

      if (textAttachments.length > 0) {
        processedContent += '\n' + textAttachments.join('\n');
      }
      return processedContent;
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