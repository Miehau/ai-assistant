import type { Message, Attachment } from '$lib/types';
import { IntentAnalysisService } from './intentAnalysis';
import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from '../openai';
import type { Model } from '$lib/types/models';
import { conversationService } from '../conversation';
import type { Tool } from './tools/types';
import { webFetcher } from './tools/webFetcher';
import { formatMessages } from '../messageFormatting';
import { LangfuseService } from '../LangfuseService';
import { v4 as uuidv4 } from 'uuid';

export class OrchestratorService {
  private intentAnalysis: IntentAnalysisService;
  private currentController: AbortController | null = null;
  private tools: Record<string, Tool>;
  private langfuse: LangfuseService;
  private state: {
    systemPrompt: string; // current system prompt
    messages: Message[];

    currentStage: Stage; // stage on which system prompt depends
    currentStep: number;
  }

  constructor() {
    // Initialize with null, will be set when we get the API key
    this.intentAnalysis = null;
    this.langfuse = new LangfuseService();
    this.tools = [webFetcher].reduce((acc, tool) => ({
      ...acc,
      [tool.name]: tool
    }), {} as Record<string, Tool>);
    this.state = {
      systemPrompt: '',
      messages: [],
      currentStage: 'initial',
      currentStep: 0
    };
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

      let trace = this.langfuse.createTrace({ id: uuidv4(), name: content, sessionId: conversation.id });
      let conversationHistory = await conversationService.getAPIHistory(conversation.id);
      this.langfuse.createEvent(trace, "Get conversation history", conversationHistory);
      console.log('Conversation history:', conversationHistory);
      // Step 1: Process audio attachments and get transcripts
      const processedAttachments = await this.processAttachments(attachments, content, null);
      // Step 2: Prepare the content by adding transcripts
      let processedContent = content;
      processedContent = appendAudioTranscripts(processedAttachments, processedContent);
      let finalResponse = '';

      let maxIterations = 4;
      let iterations = 0;
      // That's where the magic happens
      do {
        console.log(conversationHistory);
        let currentIntentSpan = this.langfuse.createGeneration(trace, "Analyze intent", processedContent);
        let currentIntent = await this.intentAnalysis.analyzeIntent(processedContent, conversationHistory);
        this.langfuse.finalizeGeneration(currentIntentSpan, currentIntent, 'gpt-4o-mini', { promptTokens: 150, completionTokens: 150, totalTokens: 300 });
        console.log('Processed content:', processedContent);
        conversationHistory.push({
          role: 'assistant',
          content: "Your inner dialog and intent analysis: " + JSON.stringify(currentIntent)
        });
        if (currentIntent.intent_type === 'tool_call') {
          let tool = this.tools[currentIntent.tool || ''];
          let toolSpan = this.langfuse.createGeneration(trace, "Execute tool", currentIntent.params);
          let toolResult = await tool.execute(currentIntent.params);
          this.langfuse.finalizeGeneration(toolSpan, toolResult.result, 'gpt-4o-mini', { promptTokens: 150, completionTokens: 150, totalTokens: 300 });
          console.log(toolResult);
          conversationHistory.push({ role: 'assistant', content: `Here is a response from ${tool.name} call: ${toolResult.result}` || '' });
          finalResponse = toolResult.result || '';
        } else {
          break;
        }
        iterations++;
      } while (true && iterations < maxIterations)

      await Promise.all([
        conversationService.saveMessage('user', content, []),
        conversationService.saveMessage('assistant', finalResponse || '', [])
      ]);

      // For all other intents, delegate to the regular chat service
      const apiKey = await this.getApiKeyForProvider('openai');
      const openAIService = new OpenAIService(apiKey);
      let chatCompletionSpan = this.langfuse.createGeneration(trace, "Chat completion", processedContent);
      const response = await openAIService.createChatCompletion(
        model,
        await formatMessages(conversationHistory, this.createMessage(processedContent, processedAttachments), systemPrompt),
        true,
        onStreamResponse,
        this.currentController.signal
      );
      this.langfuse.finalizeGeneration(chatCompletionSpan, response, model, { promptTokens: 150, completionTokens: 150, totalTokens: 300 });
      await this.langfuse.finalizeTrace(trace, processedContent, response);
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

  private async processAttachments(attachments: Attachment[], content: string, trace: any): Promise<Attachment[]> {
    const processedAttachments = [...attachments];

    for (const attachment of processedAttachments) {
      if (attachment.attachment_type.startsWith("audio") && !attachment.transcript) {
        try {
          const apiKey = await this.getApiKeyForProvider('openai');
          const openAIService = new OpenAIService(apiKey);
          let processAttachmentsSpan = this.langfuse.createGeneration(trace, "Transcribe audio", attachments);
          const transcript = await openAIService.transcribeAudio(attachment.data, content);
          this.langfuse.finalizeGeneration(processAttachmentsSpan, transcript, 'whisper-3', { promptTokens: 0, completionTokens: 0, totalTokens: 0 });
          attachment.transcript = transcript;
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