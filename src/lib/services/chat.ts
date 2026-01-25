import { invoke } from '@tauri-apps/api/tauri';
import { OpenAIService } from './openai';
import { CustomProviderService } from './customProvider';
import type { Message, Attachment, ConversationUsageSummary, CustomBackend } from '$lib/types';
import { conversationService } from './conversation';
import { modelService } from '$lib/models/modelService';
import type { Model } from '$lib/types/models';
import { formatMessages } from './messageFormatting';
import { AnthropicService } from './anthropic';
import { DeepSeekService } from './deepseek';
import { calculateCost } from '$lib/utils/costCalculator';
import { branchService } from './branchService';
import { currentConversationUsage } from '$lib/stores/tokenUsage';
import { LLMService } from './base/LLMService';
import type { LLMMessage } from '$lib/types/llm';
import { v4 as uuidv4 } from 'uuid';

/**
 * Create LLM service instance based on provider
 */
function createLLMService(provider: string, apiKey: string): LLMService {
  switch (provider.toLowerCase()) {
    case 'openai':
      return new OpenAIService(apiKey);
    case 'anthropic':
      return new AnthropicService(apiKey);
    case 'deepseek':
      return new DeepSeekService(apiKey);
    default:
      throw new Error(`Unsupported provider: ${provider}`);
  }
}

export class ChatService {
  private streamResponse = true;
  private currentController: AbortController | null = null;
  private currentBranchId: string | null = null;
  private lastMessageId: string | null = null;
  private readonly knownProviders = ['openai', 'anthropic', 'deepseek', 'custom', 'ollama', 'claude_cli'] as const;

  private async getApiKeyForProvider(provider: string): Promise<string> {
    const apiKey = await invoke<string | null>('get_api_key', { provider });
    if (!apiKey) {
      throw new Error(`No API key found for provider: ${provider}`);
    }
    return apiKey;
  }

  private normalizeModelName(name: string): string {
    return name.trim().toLowerCase();
  }

  private findModelByName(models: Model[], modelName: string): Model | undefined {
    const needle = this.normalizeModelName(modelName);
    return models.find((m) => {
      const modelMatch = this.normalizeModelName(m.model_name) === needle;
      const nameMatch = m.name ? this.normalizeModelName(m.name) === needle : false;
      return modelMatch || nameMatch;
    });
  }

  private createVirtualModel(provider: string, modelName: string, customBackendId?: string): Model {
    return {
      provider,
      model_name: modelName,
      name: modelName,
      enabled: true,
      custom_backend_id: customBackendId,
    };
  }

  private inferModelFromName(
    modelName: string,
    registryModels: Model[],
    backends: CustomBackend[]
  ): Model | null {
    const separatorPattern = /[|\/:@-]|\u2022/;
    const parts = modelName
      .split(separatorPattern)
      .map((part) => part.trim())
      .filter(Boolean);

    const provider = parts
      .map((part) => part.toLowerCase())
      .find((part) => this.knownProviders.includes(part as (typeof this.knownProviders)[number]));

    if (!provider) {
      return null;
    }

    const modelPart =
      parts.find((part) => part.toLowerCase() !== provider) || modelName;

    if (provider === 'custom') {
      const backend = backends.find((b) => this.normalizeModelName(b.name) === this.normalizeModelName(modelPart));
      if (backend) {
        return this.createVirtualModel('custom', backend.name, backend.id);
      }
    }

    const registryMatch = this.findModelByName(registryModels, modelPart);
    if (registryMatch) {
      return registryMatch;
    }

    return this.createVirtualModel(provider, modelPart);
  }

  private async getModelInfo(modelName: string): Promise<Model> {
    const models = await invoke<Model[]>('get_models');
    const registryModels = modelService.getAvailableModelsWithCapabilities();

    let selectedModel =
      this.findModelByName(models, modelName) ||
      this.findModelByName(registryModels, modelName);

    if (selectedModel) {
      return selectedModel;
    }

    console.log(`Model ${modelName} not found in database or registry, checking custom backends`);
    const backends = await invoke<CustomBackend[]>('get_custom_backends');
    const backend = backends.find((b) => b.name === modelName);
    if (backend) {
      console.log(`Found custom backend ${modelName}`);
      return this.createVirtualModel('custom', backend.name, backend.id);
    }

    const inferred = this.inferModelFromName(modelName, registryModels, backends);
    if (inferred) {
      console.warn(`Inferred model from name: ${modelName} -> ${inferred.model_name} (${inferred.provider})`);
      return inferred;
    }

    const fallback = models.find((m) => m.enabled) || registryModels.find((m) => m.enabled);
    if (fallback) {
      console.warn(`Falling back to model ${fallback.model_name} (${fallback.provider}) for missing model: ${modelName}`);
      return fallback;
    }

    throw new Error(`Model ${modelName} not found in database, registry, or custom backends`);
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
        { id: uuidv4(), type: "sent" as "sent", content: 'Title generation' }, // Dummy message
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
      id: uuidv4(),
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

            await invoke('save_message_usage', {
              input: {
                message_id: savedAssistantMessageId,
                model_name: selectedModel.model_name,
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.prompt_tokens + usage.completion_tokens,
                estimated_cost: cost
              }
            });

            const updatedUsage = await invoke<ConversationUsageSummary>('update_conversation_usage', {
              conversationId: conversation.id
            });

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
          // Audio transcription currently only supported by OpenAI
          const apiKey = await this.getApiKeyForProvider('openai');
          const llmService = createLLMService('openai', apiKey);
          attachment.transcript = await (llmService as any).transcribeAudio(attachment.data, content);
        } catch (error) {
          console.error('Failed to transcribe audio:', error);
          attachment.transcript = '[Transcription failed]';
        }
      }
    }

    return processedAttachments;
  }

  /**
   * Get custom backend configuration by ID
   */
  private async getCustomBackend(backendId: string): Promise<CustomBackend | null> {
    try {
      return await invoke<CustomBackend | null>('get_custom_backend', { id: backendId });
    } catch (error) {
      console.error(`Failed to get custom backend ${backendId}:`, error);
      return null;
    }
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

    // Handle custom provider - look up backend configuration
    if (model.provider === 'custom') {
      if (!model.custom_backend_id) {
        throw new Error('Custom model is missing backend configuration');
      }

      const backend = await this.getCustomBackend(model.custom_backend_id);
      if (!backend) {
        throw new Error(`Custom backend not found: ${model.custom_backend_id}`);
      }

      const customService = CustomProviderService.fromBackend(backend);
      return customService.createChatCompletion(
        model.model_name,
        backend.url,
        formattedMessages,
        streamResponse,
        onStreamResponse,
        signal
      );
    }

    // Get API key for provider
    const apiKey = await this.getApiKeyForProvider(model.provider);
    if (!apiKey) {
      throw new Error(`No API key found for provider: ${model.provider}`);
    }

    // Create LLM service using factory pattern - works for any provider!
    const llmService = createLLMService(model.provider, apiKey);

    // Use the legacy createChatCompletion method which supports streaming
    // Note: These methods are marked as deprecated but still needed for backward compatibility
    // All provider services (OpenAI, Anthropic, DeepSeek) have this method
    return (llmService as any).createChatCompletion(
      model.model_name,
      formattedMessages,
      streamResponse,
      onStreamResponse,
      signal
    );
  }

  async transcribeAudio(base64Audio: string, prompt: string = ''): Promise<string> {
    // Audio transcription currently only supported by OpenAI
    const apiKey = await this.getApiKeyForProvider('openai');
    const llmService = createLLMService('openai', apiKey);
    return (llmService as any).transcribeAudio(base64Audio, prompt);
  }
}

export const chatService = new ChatService();
