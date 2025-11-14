import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from './openai';
import { customProviderService } from './customProvider';
import type { Message, Attachment, ConversationUsageSummary } from '$lib/types';
import { conversationService } from './conversation';
import { modelService } from '$lib/models/modelService';
import type { Model } from '$lib/types/models';
import { formatMessages } from './messageFormatting';
import { AnthropicService } from './anthropic';
import { DeepSeekService } from './deepseek';
import { calculateCost } from '$lib/utils/costCalculator';
import { branchService } from './branchService';
import { currentConversationUsage } from '$lib/stores/tokenUsage';

// Register tools on module load
import { toolRegistry } from './toolRegistry';
import { respondTool } from '$lib/tools/respond';
import { apiCallTool } from '$lib/tools/apiCall';
import { dbSearchTool } from '$lib/tools/dbSearch';
import { orchestrator } from './orchestrator';
import type { AgentResult } from '$lib/types/agent';

toolRegistry.register(respondTool);
toolRegistry.register(apiCallTool);
toolRegistry.register(dbSearchTool);

export class ChatService {
  private streamResponse = true;
  private currentController: AbortController | null = null;
  private currentBranchId: string | null = null;
  private lastMessageId: string | null = null;

  private async getApiKeyForProvider(provider: string): Promise<string> {
    const apiKey = await invoke<string | null>('get_api_key', { provider });
    if (!apiKey) {
      throw new Error(`No API key found for provider: ${provider}`);
    }
    return apiKey;
  }

  private async getModelInfo(modelName: string): Promise<Model> {
    // First try to get the model from the database
    const models = await invoke<Model[]>('get_models');
    let selectedModel = models.find((m: Model) => m.model_name === modelName);
    
    // If not found in the database, try to get it from the registry and add it
    if (!selectedModel) {
      console.log(`Model ${modelName} not found in database, checking registry`);
      const registryModels = modelService.getAvailableModelsWithCapabilities();
      const registryModel = registryModels.find((m: Model) => m.model_name === modelName);
      
      if (registryModel) {
        console.log(`Found model ${modelName} in registry, adding to database`);
        // Add the model to the database
        // await modelService.addModel({
        //   provider: registryModel.provider,
        //   model_name: registryModel.model_name,
        //   enabled: true,
        //   url: '',
        //   deployment_name: ''
        // });
        
        // Return the registry model
        return registryModel;
      }
      
      throw new Error(`Model ${modelName} not found in database or registry`);
    }
    
    return selectedModel;
  }

  /**
   * Gets the default model to use for operations like title generation
   * @returns A promise that resolves to the model name
   */
  async getDefaultModel(): Promise<string> {
    try {
      const models = await invoke<Model[]>('get_models');
      const enabledModels = models.filter(model => model.enabled);
      
      // Prefer OpenAI models for title generation as they're good at this task
      const openaiModel = enabledModels.find(m => m.provider === 'openai');
      if (openaiModel) {
        console.log('Using OpenAI model for title generation:', openaiModel.model_name);
        return openaiModel.model_name;
      }
      
      // Fall back to any enabled model
      if (enabledModels.length > 0) {
        console.log('Using fallback model for title generation:', enabledModels[0].model_name);
        return enabledModels[0].model_name;
      }
      
      throw new Error('No enabled models found');
    } catch (error) {
      console.error('Failed to get default model:', error);
      throw error;
    }
  }

  /**
   * Generates a completion using the specified messages and model
   * @param messages The messages to use for the completion
   * @param modelName The name of the model to use
   * @returns A promise that resolves to the generated text
   */
  async generateCompletion(messages: any[], modelName: string): Promise<string> {
    try {
      const model = await this.getModelInfo(modelName);
      
      // Use a temporary controller that we'll discard after this operation
      const controller = new AbortController();
      
      let responseText = '';
      const collectResponse = (chunk: string) => {
        responseText += chunk;
      };
      
      await this.createChatCompletion(
        model,
        [], // No history needed for title generation
        { type: "sent" as "sent", content: 'Title generation' }, // Dummy message
        'You are a helpful assistant.',
        false, // Don't stream for title generation
        collectResponse,
        controller.signal,
        messages // Pass the messages directly
      );
      
      return responseText;
    } catch (error) {
      console.error('Failed to generate completion:', error);
      throw error;
    }
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

  /**
   * Initialize branch context when loading an existing conversation
   */
  async initializeBranchContext(conversationId: string) {
    try {
      // Get or create main branch
      const mainBranch = await branchService.getOrCreateMainBranch(conversationId);
      this.currentBranchId = mainBranch.id;

      // Get all messages in this conversation to find the last one
      const history = await conversationService.getDisplayHistory(conversationId);
      if (history.length > 0) {
        const lastMessage = history[history.length - 1];
        this.lastMessageId = lastMessage.id || null;
      } else {
        this.lastMessageId = null;
      }

      console.log('Branch context initialized:', {
        branchId: this.currentBranchId,
        lastMessageId: this.lastMessageId
      });
    } catch (error) {
      console.warn('Failed to initialize branch context:', error);
    }
  }

  /**
   * Reset branch context (call when starting a new conversation)
   */
  resetBranchContext() {
    this.currentBranchId = null;
    this.lastMessageId = null;
  }

  async handleSendMessage(
    content: string,
    model: string,
    onStreamResponse: (chunk: string) => void,
    systemPrompt?: string,
    attachments: Attachment[] = [],
    userMessageId?: string,
    assistantMessageId?: string,
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
        processedContent += '\n' + audioTranscripts.join('\n');
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
      const aiResponse = await this.createChatCompletion(
        selectedModel,
        history,
        message,
        systemPrompt || "You are a helpful AI assistant.",
        this.streamResponse,
        onStreamResponse,
        this.currentController.signal
      );

      this.currentController = null;

      // Extract response content and usage
      const modelResponse = typeof aiResponse === 'string' ? aiResponse : aiResponse.content;
      const usage = typeof aiResponse === 'object' ? aiResponse.usage : undefined;

      // Step 7: Get or create main branch for this conversation
      if (!this.currentBranchId) {
        const mainBranch = await branchService.getOrCreateMainBranch(conversation.id);
        this.currentBranchId = mainBranch.id;
      }

      // Step 8: Save both messages to the conversation with their IDs
      const [savedUserMessageId, savedAssistantMessageId] = await Promise.all([
        conversationService.saveMessage('user', message.content, message.attachments || [], undefined, userMessageId),
        conversationService.saveMessage('assistant', modelResponse, [], undefined, assistantMessageId)
      ]);

      // Step 9: Create tree nodes for both messages in parallel
      try {
        // Create both tree nodes simultaneously since they're independent
        await Promise.all([
          branchService.createMessageTreeNode(
            savedUserMessageId,
            this.lastMessageId,
            this.currentBranchId,
            false // Not a branch point by default
          ),
          branchService.createMessageTreeNode(
            savedAssistantMessageId,
            savedUserMessageId,
            this.currentBranchId,
            false // Not a branch point by default
          )
        ]);

        // Update last message ID to assistant message
        this.lastMessageId = savedAssistantMessageId;
      } catch (branchError) {
        // Don't fail the chat if branch tracking fails
        console.warn('Failed to create message tree nodes:', branchError);
      }

      // Step 10: Track usage in background (non-blocking)
      if (usage && savedAssistantMessageId) {
        // Run usage tracking in background without blocking the response
        Promise.resolve().then(async () => {
          try {
            const cost = calculateCost(
              selectedModel.model_name,
              usage.prompt_tokens,
              usage.completion_tokens
            );

            // Save usage data and update summary in parallel
            const [, updatedUsage] = await Promise.all([
              invoke('save_message_usage', {
                input: {
                  message_id: savedAssistantMessageId,
                  model_name: selectedModel.model_name,
                  prompt_tokens: usage.prompt_tokens,
                  completion_tokens: usage.completion_tokens,
                  total_tokens: usage.prompt_tokens + usage.completion_tokens,
                  estimated_cost: cost
                }
              }),
              invoke<ConversationUsageSummary>('update_conversation_usage', {
                conversationId: conversation.id
              })
            ]);

            // Update the store to refresh the UI
            currentConversationUsage.set(updatedUsage);
          } catch (usageError) {
            // Don't fail the chat if usage tracking fails
            console.warn('Failed to save usage data:', usageError);
          }
        });
      }

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
    signal: AbortSignal,
    customMessages?: any[]
  ): Promise<string | { content: string; usage?: { prompt_tokens: number; completion_tokens: number } }> {
    
    // Use custom messages if provided, otherwise format the history and message
    const formattedMessages = customMessages || await formatMessages(history, message, systemPrompt);

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
    
    if (model.provider === 'deepseek') {
      const apiKey = await this.getApiKeyForProvider(model.provider);
      const deepSeekService = new DeepSeekService(apiKey);
      return deepSeekService.createChatCompletion(
        model.model_name,
        formattedMessages,
        streamResponse,
        onStreamResponse,
        signal
      );
    }
    
    throw new Error(`Unsupported provider: ${model.provider}`);
  }

  /**
   * Handles sending a message with agent tool use capabilities
   * This uses the dual-model architecture: agent LLM executes tools, user-facing LLM presents results
   */
  async handleSendMessageWithAgent(
    content: string,
    onStreamResponse: (chunk: string) => void,
    onAgentActivity?: (activity: { status: string; toolsUsed?: any[]; iterations?: number }) => void,
    attachments: Attachment[] = [],
    userMessageId?: string,
    assistantMessageId?: string
  ) {
    try {
      this.currentController = new AbortController();

      // Step 1: Process attachments
      const processedAttachments = await this.processAttachments(attachments, content);

      // Step 2: Prepare content with transcripts
      let processedContent = content;
      const audioTranscripts = processedAttachments
        .filter((att) => att.attachment_type.startsWith('audio') && att.transcript)
        .map((att) => `[Audio Transcript]: ${att.transcript}`);

      if (audioTranscripts.length > 0) {
        processedContent += '\n' + audioTranscripts.join('\n');
      }

      // Step 3: Get or create conversation
      const conversation =
        conversationService.getCurrentConversation() ??
        (await conversationService.setCurrentConversation(null));
      const history = await conversationService.getAPIHistory(conversation.id);

      // Step 4: Get or create main branch
      if (!this.currentBranchId) {
        const mainBranch = await branchService.getOrCreateMainBranch(conversation.id);
        this.currentBranchId = mainBranch.id;
      }

      // Step 5: Use orchestrator to handle the message with agent
      let fullResponse = '';
      let agentResult: AgentResult | undefined;

      for await (const event of orchestrator.handleUserMessage(
        processedContent,
        conversation.id,
        history
      )) {
        if (event.type === 'agent_status') {
          onAgentActivity?.({ status: event.data.message });
        } else if (event.type === 'agent_complete') {
          agentResult = {
            success: event.data.success,
            toolsUsed: event.data.toolsUsed,
            iterations: event.data.iterations,
            metadata: event.data.metadata
          };
          onAgentActivity?.({
            status: 'complete',
            toolsUsed: event.data.toolsUsed,
            iterations: event.data.iterations
          });
        } else if (event.type === 'stream_chunk') {
          fullResponse += event.data;
          onStreamResponse(event.data);
        } else if (event.type === 'error') {
          throw new Error(event.data.message);
        }
      }

      this.currentController = null;

      // Step 6: Save messages to conversation
      const [savedUserMessageId, savedAssistantMessageId] = await Promise.all([
        conversationService.saveMessage(
          'user',
          processedContent,
          processedAttachments || [],
          undefined,
          userMessageId
        ),
        conversationService.saveMessage('assistant', fullResponse, [], undefined, assistantMessageId)
      ]);

      // Step 7: Create tree nodes for branching
      try {
        await Promise.all([
          branchService.createMessageTreeNode(
            savedUserMessageId,
            this.lastMessageId,
            this.currentBranchId,
            false
          ),
          branchService.createMessageTreeNode(
            savedAssistantMessageId,
            savedUserMessageId,
            this.currentBranchId,
            false
          )
        ]);

        this.lastMessageId = savedAssistantMessageId;
      } catch (branchError) {
        console.warn('Failed to create message tree nodes:', branchError);
      }

      // Step 8: Track usage if agent was used
      if (agentResult && savedAssistantMessageId) {
        Promise.resolve().then(async () => {
          try {
            const totalTokens = agentResult.metadata.tokensUsed;
            const cost = agentResult.metadata.cost || 0;

            await Promise.all([
              invoke('save_message_usage', {
                input: {
                  message_id: savedAssistantMessageId,
                  model_name: agentResult.metadata.modelUsed,
                  prompt_tokens: Math.floor(totalTokens * 0.6), // Estimate
                  completion_tokens: Math.floor(totalTokens * 0.4), // Estimate
                  total_tokens: totalTokens,
                  estimated_cost: cost
                }
              }),
              invoke('update_conversation_usage', {
                conversationId: conversation.id
              }).then((usage) => currentConversationUsage.set(usage))
            ]);
          } catch (usageError) {
            console.warn('Failed to save usage data:', usageError);
          }
        });
      }

      return {
        text: fullResponse,
        conversationId: conversation.id,
        agentResult
      };
    } catch (error: unknown) {
      if (error instanceof Error && error.name === 'AbortError') {
        console.log('Request was cancelled');
        return;
      }
      console.error('Failed to send chat message with agent:', error);
      throw error;
    }
  }

  async transcribeAudio(base64Audio: string, prompt: string = ''): Promise<string> {
    const apiKey = await this.getApiKeyForProvider('openai');
    const openAIService = new OpenAIService(apiKey);
    return openAIService.transcribeAudio(base64Audio, prompt);
  }
}

export const chatService = new ChatService();
