import type { Message, Attachment, APIMessage } from '$lib/types';
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
import { decidePrompt, describePrompt, finalAnswerPrompt, planPrompt, reflectionPrompt } from './prompts';
import { finalAnswer } from './tools/finalAnswer';

export type OrchestratorState = {
  actionsTaken: { name: string, payload: string, reflection: string, result: string, tool: Tool }[];
  availableTools: Tool[];
  systemPrompt: string; // current system prompt
  plan: string;
  messages: APIMessage[];
  currentStage: string; // stage on which system prompt depends
  currentStep: number;
  activeTool: Tool | null;
  trace: any
}

export class OrchestratorService {
  private intentAnalysis!: IntentAnalysisService;
  private currentController: AbortController | null = null;
  private tools: Record<string, Tool>;
  private langfuse: LangfuseService;
  private state!: OrchestratorState;
  private openAIService!: OpenAIService;

  constructor() {
    this.langfuse = new LangfuseService();
    this.tools = [webFetcher, finalAnswer].reduce((acc, tool) => ({
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
      const modelInfo = await this.getModelInfo(model);
      const apiKey = await this.getApiKeyForProvider(modelInfo.provider);
      this.openAIService = new OpenAIService(apiKey);
      this.state = {
        actionsTaken: [],
        availableTools: [],
        systemPrompt: '',
        plan: '',
        messages: [],
        currentStage: 'init',
        currentStep: 0,
        activeTool: null,
        trace: null
      }

      let trace = this.langfuse.createTrace({ id: uuidv4(), name: content, sessionId: conversation.id });
      this.state.trace = trace;
      let conversationHistory = await conversationService.getAPIHistory(conversation.id);
      this.langfuse.createEvent(trace, "Get conversation history", conversationHistory);
      // Step 1: Process audio attachments and get transcripts
      const processedAttachments = await this.processAttachments(attachments, content, null);
      // Step 2: Prepare the content by adding transcripts
      let processedContent = content;
      processedContent = appendAudioTranscripts(processedAttachments, processedContent);
      this.state.availableTools = Object.values(this.tools);
      this.state.messages.push({ role: 'user', content: processedContent });

      let maxIterations = 4;
      // That's where the magic happens
      while(this.state.activeTool !== 'final_answer' && this.state.currentStep < maxIterations) {
        await this.plan(this.state, trace);
        await this.decide(this.state, trace);
        console.log('After decide state:', this.state);
        if (this.state.activeTool?.name === 'final_answer') break;
        await this.describe(this.state, trace); // describe how to use the tool
        await this.execute(this.state, trace); // execute the tool
        await this.reflect(this.state, trace); // reflect on the result
        this.state.currentStep++;
      }

      let formattedMessages = this.state.messages.map(m => ({ role: m.role, content: m.content }));
      console.log('Final answer state:', this.state);
      let finalAnswerSpan = this.langfuse.createGeneration(trace, `Final answer ${this.state.currentStep}`, formattedMessages);
      let finalAnswer = await this.openAIService.completion(
        model,
        [{ role: 'system', content: finalAnswerPrompt(this.state) }, ...formattedMessages],
        this.currentController!.signal
      );
      this.langfuse.finalizeGeneration(finalAnswerSpan, finalAnswer.message.message, model, { promptTokens: finalAnswer.usage.promptTokens, completionTokens: finalAnswer.usage.completionTokens, totalTokens: finalAnswer.usage.totalTokens });

      this.langfuse.createEvent(trace, "Saving final answer", finalAnswer.message.message);
      await Promise.all([
        conversationService.saveMessage('user', content, []),
        conversationService.saveMessage('assistant', finalAnswer.message.message || '', [])
      ]);
      this.langfuse.finalizeTrace(trace, finalAnswer.message.message, finalAnswer.message.message);
      onStreamResponse(finalAnswer.message.message);
      return finalAnswer.message.message;
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

  async plan(state: any, trace: any) {
    state.currentStage = 'plan';
    let planSpan = this.langfuse.createGeneration(trace, `Plan ${state.currentStep}`, state.messages);
    console.log('Plan state:', state);
    const response = await this.openAIService.completion(
      state.model,
      [{ role: 'system', content: planPrompt(state) }, ...state.messages],
      this.currentController!.signal
    );
    console.log('Plan response:', response);
    state.plan = response.message.message;
    this.langfuse.finalizeGeneration(planSpan, response.message.message, state.model, { promptTokens: response.usage.promptTokens, completionTokens: response.usage.completionTokens, totalTokens: response.usage.totalTokens });
    state.messages.push({ role: 'assistant', content: response.message.message });
    return response;
  }

  async decide(state: any, trace: any) {
    state.currentStage = 'decide';
    let decideSpan = this.langfuse.createGeneration(trace, `Decide ${state.currentStep}`, state.messages);
    console.log('Decide state:', state);
    console.log([{ role: 'system', content: decidePrompt(state) }, ...state.messages])
    const response = await this.openAIService.completion(
      state.model,
      [{ role: 'system', content: decidePrompt(state) }, ...state.messages],
      this.currentController!.signal
    );
    console.log('Decide response:', response);
    this.langfuse.finalizeGeneration(decideSpan, response.message.message, state.model, { promptTokens: response.usage.promptTokens, completionTokens: response.usage.completionTokens, totalTokens: response.usage.totalTokens });
    state.messages.push({ role: 'assistant', content: response.message.message });
    state.activeTool = this.tools[JSON.parse(response.message.message).tool];
    return response;
  }

  async describe(state: any, trace: any) {
    state.currentStage = 'describe';
    let describeSpan = this.langfuse.createGeneration(trace, `Describe ${state.currentStep}`, state.messages);
    console.log('Describe state:', state);
    const response = await this.openAIService.completion(
      state.model,
      [{ role: 'system', content: describePrompt(state) }, ...state.messages],
      this.currentController!.signal
    );
    console.log('Describe response:', response);
    this.langfuse.finalizeGeneration(describeSpan, response.message.message, state.model, { promptTokens: response.usage.promptTokens, completionTokens: response.usage.completionTokens, totalTokens: response.usage.totalTokens });
    state.messages.push({ role: 'assistant', content: response.message.message });
    state.activeToolPayload = JSON.parse(response.message.message);
    return response;
  }

  async execute(state: any, trace: any) {
    if (!state.activeTool) {
      throw new Error('No active tool to execute');
    }
    state.currentStage = 'execute';
    let executeSpan = this.langfuse.createSpan(trace, `Execute ${state.currentStep}`, state.messages);
    const response = await state.activeTool.execute(state.activeToolPayload);
    this.langfuse.finalizeSpan(executeSpan, `Execute ${state.activeTool.name}`, state.activeToolPayload, response);
    this.state.actionsTaken.push({ name: state.activeTool.name, payload: JSON.stringify(state.activeToolPayload), reflection: '', result: response.result, tool: state.activeTool });
    return response;
  }

  async reflect(state: any, trace: any) {
    state.currentStage = 'reflect';
    let reflectSpan = this.langfuse.createGeneration(trace, `Reflect ${state.currentStep}`, state.messages);
    const response = await this.openAIService.completion(
      state.model,
      [{ role: 'system', content: reflectionPrompt(state) }, ...state.messages],
      this.currentController!.signal
    );
    state.actionsTaken[state.actionsTaken.length - 1].reflection = response.message.message;
    this.langfuse.finalizeGeneration(reflectSpan, response.message.message, state.model, { promptTokens: response.usage.promptTokens, completionTokens: response.usage.completionTokens, totalTokens: response.usage.totalTokens });
    return response;
  }
}

export const orchestratorService = new OrchestratorService(); 

