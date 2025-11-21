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
import { LLMService } from '../base/LLMService';
import { AnthropicService } from '../anthropic';
import { planSchema, decideSchema, createToolParametersSchema } from './schemas';
import type { LLMMessage } from '$lib/types/llm';
import { AGENT_CONFIG } from '$lib/config/agent';

export type OrchestratorState = {
  actionsTaken: { name: string, payload: string, reflection: string, result: string, tool: Tool }[];
  availableTools: Tool[];
  systemPrompt: string; // current system prompt
  plan: string;
  messages: APIMessage[];
  currentStage: string; // stage on which system prompt depends
  currentStep: number;
  activeTool: Tool | null;
  activeToolPayload?: any;
  model: string;
  trace: any
}

export class OrchestratorService {
  private intentAnalysis!: IntentAnalysisService;
  private currentController: AbortController | null = null;
  private tools: Record<string, Tool>;
  private langfuse: LangfuseService;
  private state!: OrchestratorState;
  private openAIService!: OpenAIService;
  private llmService!: LLMService;

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
      id: uuidv4(),
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

      // Initialize LLM service based on model
      await this.initializeLLMService(model);
      this.state = {
        actionsTaken: [],
        availableTools: [],
        systemPrompt: '',
        plan: '',
        messages: [],
        currentStage: 'init',
        currentStep: 0,
        activeTool: null,
        activeToolPayload: undefined,
        model: model,
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

      let maxIterations = AGENT_CONFIG.MAX_ITERATIONS;
      // That's where the magic happens
      while(this.state.activeTool?.name !== 'final_answer' && this.state.currentStep < maxIterations) {
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
        { signal: this.currentController!.signal }
      );
      this.langfuse.finalizeGeneration(finalAnswerSpan, finalAnswer.message, model, {
        promptTokens: finalAnswer.usage?.promptTokens || 0,
        completionTokens: finalAnswer.usage?.completionTokens || 0,
        totalTokens: finalAnswer.usage?.totalTokens || 0
      });

      this.langfuse.createEvent(trace, "Saving final answer", finalAnswer.message);
      await Promise.all([
        conversationService.saveMessage('user', content, []),
        conversationService.saveMessage('assistant', finalAnswer.message || '', [])
      ]);
      this.langfuse.finalizeTrace(trace, finalAnswer.message, finalAnswer.message);
      onStreamResponse(finalAnswer.message);
      return finalAnswer.message;
    } catch (error) {
      const errorContext = this.state ? {
        stage: this.state.currentStage,
        step: this.state.currentStep,
        activeTool: this.state.activeTool?.name,
        model: this.state.model
      } : 'initialization';
      console.error('Failed in orchestrator:', { error, context: errorContext });
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

  private async initializeLLMService(modelName: string): Promise<void> {
    const selectedModel = await this.getModelInfo(modelName);

    const apiKey = await this.getApiKeyForProvider(selectedModel.provider);

    switch (selectedModel.provider) {
      case 'openai':
        this.llmService = new OpenAIService(apiKey);
        break;
      case 'anthropic':
        this.llmService = new AnthropicService(apiKey);
        break;
      default:
        throw new Error(`Unsupported provider: ${selectedModel.provider}`);
    }
  }

  async plan(state: OrchestratorState, trace: any) {
    state.currentStage = 'plan';
    let planSpan = this.langfuse.createGeneration(trace, `Plan ${state.currentStep}`, state.messages);
    console.log('Plan state:', state);

    const llmMessages: LLMMessage[] = [
      { role: 'system', content: planPrompt(state) },
      ...state.messages
    ];

    const response = await this.llmService.structuredCompletion(
      state.model,
      llmMessages,
      planSchema,
      { signal: this.currentController!.signal }
    );

    console.log('Plan response:', response);
    state.plan = response.rawResponse;
    this.langfuse.finalizeGeneration(planSpan, response.rawResponse, state.model, {
      promptTokens: response.usage?.promptTokens || 0,
      completionTokens: response.usage?.completionTokens || 0,
      totalTokens: response.usage?.totalTokens || 0
    });
    state.messages.push({ role: 'assistant', content: response.rawResponse });
    return response;
  }

  async decide(state: OrchestratorState, trace: any) {
    state.currentStage = 'decide';
    let decideSpan = this.langfuse.createGeneration(trace, `Decide ${state.currentStep}`, state.messages);
    console.log('Decide state:', state);

    const llmMessages: LLMMessage[] = [
      { role: 'system', content: decidePrompt(state) },
      ...state.messages
    ];

    const response = await this.llmService.structuredCompletion(
      state.model,
      llmMessages,
      decideSchema,
      { signal: this.currentController!.signal }
    );

    console.log('Decide response:', response);
    this.langfuse.finalizeGeneration(decideSpan, response.rawResponse, state.model, {
      promptTokens: response.usage?.promptTokens || 0,
      completionTokens: response.usage?.completionTokens || 0,
      totalTokens: response.usage?.totalTokens || 0
    });
    state.messages.push({ role: 'assistant', content: response.rawResponse });

    // NO MORE JSON.parse! Data is already validated
    state.activeTool = this.tools[response.data.tool];
    return response;
  }

  async describe(state: OrchestratorState, trace: any) {
    state.currentStage = 'describe';

    if (!state.activeTool) {
      throw new Error(`No active tool to describe at step ${state.currentStep}. Previous stage: ${state.currentStage}`);
    }

    let describeSpan = this.langfuse.createGeneration(trace, `Describe ${state.currentStep}`, state.messages);
    console.log('Describe state:', state);

    const toolParamsSchema = createToolParametersSchema(
      state.activeTool.name,
      state.activeTool.input_schema
    );

    const llmMessages: LLMMessage[] = [
      { role: 'system', content: describePrompt(state) },
      ...state.messages
    ];

    const response = await this.llmService.structuredCompletion(
      state.model,
      llmMessages,
      toolParamsSchema,
      { signal: this.currentController!.signal }
    );

    console.log('Describe response:', response);
    this.langfuse.finalizeGeneration(describeSpan, response.rawResponse, state.model, {
      promptTokens: response.usage?.promptTokens || 0,
      completionTokens: response.usage?.completionTokens || 0,
      totalTokens: response.usage?.totalTokens || 0
    });
    state.messages.push({ role: 'assistant', content: response.rawResponse });

    // NO MORE JSON.parse! Data is already validated
    state.activeToolPayload = response.data;
    return response;
  }

  async execute(state: OrchestratorState, trace: any) {
    if (!state.activeTool) {
      throw new Error(`No active tool to execute at step ${state.currentStep}. Previous stage: ${state.currentStage}`);
    }
    state.currentStage = 'execute';
    let executeSpan = this.langfuse.createSpan(trace, `Execute ${state.currentStep}`, state.messages);
    const response = await state.activeTool.execute(state.activeToolPayload);
    this.langfuse.finalizeSpan(executeSpan, `Execute ${state.activeTool.name}`, state.activeToolPayload, response);
    this.state.actionsTaken.push({ name: state.activeTool.name, payload: JSON.stringify(state.activeToolPayload), reflection: '', result: response.result, tool: state.activeTool });
    return response;
  }

  async reflect(state: OrchestratorState, trace: any) {
    state.currentStage = 'reflect';
    let reflectSpan = this.langfuse.createGeneration(trace, `Reflect ${state.currentStep}`, state.messages);
    const response = await this.openAIService.completion(
      state.model,
      [{ role: 'system', content: reflectionPrompt(state) }, ...state.messages],
      { signal: this.currentController!.signal }
    );
    state.actionsTaken[state.actionsTaken.length - 1].reflection = response.message;
    this.langfuse.finalizeGeneration(reflectSpan, response.message, state.model, {
      promptTokens: response.usage?.promptTokens || 0,
      completionTokens: response.usage?.completionTokens || 0,
      totalTokens: response.usage?.totalTokens || 0
    });
    return response;
  }
}

export const orchestratorService = new OrchestratorService(); 

