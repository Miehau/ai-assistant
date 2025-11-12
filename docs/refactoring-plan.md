# AI Frontend Refactoring Plan

**Goal:** Refactor existing code for better maintainability, testability, and extensibility WITHOUT adding new features.

**Date:** 2025-01-12
**Status:** Planning Phase

---

## Table of Contents

1. [Overview](#overview)
2. [Current Architecture Analysis](#current-architecture-analysis)
3. [Refactoring Phases](#refactoring-phases)
4. [Implementation Details](#implementation-details)
5. [Migration Strategy](#migration-strategy)
6. [Testing Approach](#testing-approach)
7. [Success Criteria](#success-criteria)

---

## Overview

### Principles
- **Zero Feature Addition**: Only restructure existing functionality
- **Maintain Compatibility**: All existing features must continue working
- **Incremental Changes**: Small, testable changes that can be reviewed individually
- **Type Safety**: Improve TypeScript usage throughout
- **Separation of Concerns**: Clear boundaries between layers

### Key Problems to Address
1. **Service coupling**: ChatService does too much (400+ lines)
2. **Type inconsistency**: Multiple overlapping message types
3. **State fragmentation**: Stores scattered across multiple files
4. **Provider logic**: Hard-coded if/else chains instead of registry
5. **Error handling**: Inconsistent and incomplete

---

## Current Architecture Analysis

### File Structure
```
src/lib/
├── services/
│   ├── chat.ts                    // 413 lines - TOO LARGE
│   ├── conversation.ts            // Has internal state - ANTI-PATTERN
│   ├── openai.ts, anthropic.ts, etc.
│   └── agent/
├── stores/
│   ├── tokenUsage.ts
│   ├── branches.ts
│   └── visibility.ts
├── components/
│   └── chat/
│       └── store.ts               // UI + business logic mixed
└── types/
    ├── types.ts                   // Main types
    ├── index.ts                   // Duplicate Message type
    └── models.ts
```

### Critical Issues

#### 1. ChatService Responsibilities (chat.ts)
```typescript
// CURRENT: One class doing everything
export class ChatService {
  // ❌ API key management
  private async getApiKeyForProvider() {...}

  // ❌ Model resolution
  private async getModelInfo() {...}

  // ❌ Message orchestration
  async handleSendMessage() {...}

  // ❌ Attachment processing
  private async processAttachments() {...}

  // ❌ Provider selection
  private async createChatCompletion() {
    if (provider === 'openai') {...}
    if (provider === 'anthropic') {...}
    if (provider === 'custom') {...}
    // More if statements...
  }

  // ❌ Branch management
  async initializeBranchContext() {...}

  // ❌ Usage tracking
  // Inline in handleSendMessage
}
```

#### 2. Type Confusion
```typescript
// src/lib/types.ts
export type Message = {
  id?: string;  // ❌ Optional causes branching bugs
  type: "sent" | "received";
  content: string;
}

// src/lib/types/index.ts
export interface Message {  // ❌ Name collision!
  content: string;
  attachments?: {...}[];
}

// Different but similar
export type APIMessage = {...}
export interface DBMessage = {...}
```

#### 3. State Management Chaos
```typescript
// Multiple stores, unclear ownership
chat/store.ts:        writable<Message[]>()
conversation.ts:      private state = writable<ConversationState>()
tokenUsage.ts:        writable<ConversationUsageSummary | null>()
branches.ts:          writable<BranchState>()
```

---

## Refactoring Phases

### Phase 1: Type System Consolidation (Days 1-2)

**Objective:** Create a coherent type hierarchy with no overlaps or ambiguity.

**Tasks:**
1. **Audit all type definitions**
   - List all Message-related types
   - Identify overlaps and conflicts
   - Document current usage

2. **Create unified type hierarchy**
   ```typescript
   // src/lib/types/message.ts (NEW)

   // Base message (always has ID)
   export interface BaseMessage {
     id: string;  // ✅ Required, not optional
     content: string;
     timestamp: number;
   }

   // Display message (UI layer)
   export interface DisplayMessage extends BaseMessage {
     type: 'sent' | 'received';
     attachments?: Attachment[];
     model?: string;
   }

   // API message (network layer)
   export interface APIMessage {
     role: 'user' | 'assistant' | 'system';
     content: string;
     attachments?: Attachment[];
   }

   // Database message (persistence layer)
   export interface DBMessage {
     id: string;
     role: 'user' | 'assistant';
     content: string;
     attachments?: Attachment[];
     timestamp: number;
   }
   ```

3. **Add type converters**
   ```typescript
   // src/lib/types/converters.ts (NEW)
   export const toDisplayMessage = (db: DBMessage): DisplayMessage => {...}
   export const toAPIMessage = (display: DisplayMessage): APIMessage => {...}
   export const toDBMessage = (display: DisplayMessage): DBMessage => {...}
   ```

4. **Update all imports**
   - Replace `import type { Message }` with specific type
   - Update function signatures
   - Run type checker

**Files to Modify:**
- `src/lib/types/index.ts` - Remove duplicate Message
- `src/lib/types/message.ts` - CREATE NEW
- `src/lib/types/converters.ts` - CREATE NEW
- `src/lib/services/chat.ts` - Update types
- `src/lib/services/conversation.ts` - Update types
- `src/lib/components/chat/store.ts` - Update types

**Success Criteria:**
- ✅ No `any` types related to messages
- ✅ No type `as` casting for messages
- ✅ Clear layer separation (Display/API/DB)
- ✅ `npm run check` passes

---

### Phase 2: Extract Provider Registry (Days 3-4)

**Objective:** Replace hard-coded provider if/else chains with pluggable registry pattern.

**Current Problem:**
```typescript
// src/lib/services/chat.ts:355-402
if (model.provider === 'openai') {
  const apiKey = await this.getApiKeyForProvider(model.provider);
  const openAIService = new OpenAIService(apiKey);
  return openAIService.createChatCompletion(...);
}

if (model.provider === 'anthropic') {
  const apiKey = await this.getApiKeyForProvider(model.provider);
  const anthropicService = new AnthropicService(apiKey);
  return anthropicService.createChatCompletion(...);
}
// ... more if statements
```

**Solution:**
```typescript
// src/lib/services/providers/types.ts (NEW)
export interface AIProvider {
  readonly name: string;
  createChatCompletion(
    modelName: string,
    messages: APIMessage[],
    streamResponse: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal
  ): Promise<string | CompletionResponse>;
}

export interface CompletionResponse {
  content: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
  };
}
```

```typescript
// src/lib/services/providers/registry.ts (NEW)
export class ProviderRegistry {
  private providers = new Map<string, AIProvider>();

  register(provider: AIProvider): void {
    this.providers.set(provider.name, provider);
  }

  get(providerName: string): AIProvider {
    const provider = this.providers.get(providerName);
    if (!provider) {
      throw new Error(`Provider not found: ${providerName}`);
    }
    return provider;
  }

  has(providerName: string): boolean {
    return this.providers.has(providerName);
  }
}

// Global registry instance
export const providerRegistry = new ProviderRegistry();
```

```typescript
// src/lib/services/providers/openai.provider.ts (NEW)
import { OpenAIService } from '../openai';
import type { AIProvider } from './types';

export class OpenAIProvider implements AIProvider {
  readonly name = 'openai';

  constructor(private apiKey: string) {}

  async createChatCompletion(...args) {
    const service = new OpenAIService(this.apiKey);
    return service.createChatCompletion(...args);
  }
}
```

```typescript
// src/lib/services/providers/index.ts (NEW)
import { providerRegistry } from './registry';
import { OpenAIProvider } from './openai.provider';
import { AnthropicProvider } from './anthropic.provider';
import { DeepSeekProvider } from './deepseek.provider';
import { CustomProvider } from './custom.provider';

// Initialize all providers
export async function initializeProviders() {
  // Providers will get API keys when instantiated
  // This is just registration, not initialization
}

export { providerRegistry };
```

**Updated ChatService:**
```typescript
// src/lib/services/chat.ts - SIMPLIFIED
private async createChatCompletion(
  model: Model,
  history: APIMessage[],
  message: DisplayMessage,
  systemPrompt: string,
  streamResponse: boolean,
  onStreamResponse: (chunk: string) => void,
  signal: AbortSignal,
  customMessages?: APIMessage[]
): Promise<string | CompletionResponse> {
  const formattedMessages = customMessages || await formatMessages(history, message, systemPrompt);

  // ✅ Simple registry lookup instead of if/else chains
  const provider = providerRegistry.get(model.provider);
  return provider.createChatCompletion(
    model.model_name,
    formattedMessages,
    streamResponse,
    onStreamResponse,
    signal
  );
}
```

**Files to Create:**
- `src/lib/services/providers/types.ts`
- `src/lib/services/providers/registry.ts`
- `src/lib/services/providers/openai.provider.ts`
- `src/lib/services/providers/anthropic.provider.ts`
- `src/lib/services/providers/deepseek.provider.ts`
- `src/lib/services/providers/custom.provider.ts`
- `src/lib/services/providers/index.ts`

**Files to Modify:**
- `src/lib/services/chat.ts` - Use registry
- Startup code to initialize registry

**Success Criteria:**
- ✅ No if/else chains for provider selection
- ✅ Easy to add new providers
- ✅ All existing providers work
- ✅ Tests pass

---

### Phase 3: Break Down ChatService (Days 5-7)

**Objective:** Split the 400+ line ChatService into focused, testable services.

**Target Structure:**
```
src/lib/services/
├── chat/
│   ├── ChatCoordinator.ts         // Main orchestration
│   ├── AttachmentProcessor.ts     // Handle attachments
│   ├── ApiKeyManager.ts           // API key operations
│   ├── ModelResolver.ts           // Model lookup logic
│   └── index.ts
├── providers/                     // From Phase 2
└── [existing services...]
```

**1. Extract AttachmentProcessor**
```typescript
// src/lib/services/chat/AttachmentProcessor.ts (NEW)
export class AttachmentProcessor {
  /**
   * Process attachments, transcribe audio if needed
   */
  async processAttachments(
    attachments: Attachment[],
    content: string
  ): Promise<Attachment[]> {
    const processed = [...attachments];

    for (const attachment of processed) {
      if (attachment.attachment_type.startsWith("audio") && !attachment.transcript) {
        try {
          attachment.transcript = await this.transcribeAudio(
            attachment.data,
            content
          );
        } catch (error) {
          console.error('Failed to transcribe audio:', error);
          attachment.transcript = '[Transcription failed]';
        }
      }
    }

    return processed;
  }

  private async transcribeAudio(
    base64Audio: string,
    prompt: string
  ): Promise<string> {
    // Get API key and use OpenAI service
    const apiKey = await invoke<string>('get_api_key', { provider: 'openai' });
    if (!apiKey) throw new Error('OpenAI API key not found');

    const openAIService = new OpenAIService(apiKey);
    return openAIService.transcribeAudio(base64Audio, prompt);
  }
}
```

**2. Extract ApiKeyManager**
```typescript
// src/lib/services/chat/ApiKeyManager.ts (NEW)
import { invoke } from '@tauri-apps/api/tauri';

export class ApiKeyManager {
  private keyCache = new Map<string, string>();

  async getApiKey(provider: string): Promise<string> {
    // Check cache first
    if (this.keyCache.has(provider)) {
      return this.keyCache.get(provider)!;
    }

    const apiKey = await invoke<string | null>('get_api_key', { provider });
    if (!apiKey) {
      throw new Error(`No API key found for provider: ${provider}`);
    }

    this.keyCache.set(provider, apiKey);
    return apiKey;
  }

  clearCache(): void {
    this.keyCache.clear();
  }
}
```

**3. Extract ModelResolver**
```typescript
// src/lib/services/chat/ModelResolver.ts (NEW)
import { invoke } from '@tauri-apps/api/tauri';
import { modelService } from '$lib/models/modelService';
import type { Model } from '$lib/types/models';

export class ModelResolver {
  /**
   * Get model info from database or registry
   */
  async resolveModel(modelName: string): Promise<Model> {
    // First try to get the model from the database
    const models = await invoke<Model[]>('get_models');
    let selectedModel = models.find((m: Model) => m.model_name === modelName);

    // If not found in the database, try to get it from the registry
    if (!selectedModel) {
      console.log(`Model ${modelName} not found in database, checking registry`);
      const registryModels = modelService.getAvailableModelsWithCapabilities();
      const registryModel = registryModels.find((m: Model) => m.model_name === modelName);

      if (registryModel) {
        console.log(`Found model ${modelName} in registry`);
        return registryModel;
      }

      throw new Error(`Model ${modelName} not found in database or registry`);
    }

    return selectedModel;
  }

  /**
   * Gets the default model to use for operations like title generation
   */
  async getDefaultModel(): Promise<string> {
    const models = await invoke<Model[]>('get_models');
    const enabledModels = models.filter(model => model.enabled);

    // Prefer OpenAI models for title generation
    const openaiModel = enabledModels.find(m => m.provider === 'openai');
    if (openaiModel) {
      return openaiModel.model_name;
    }

    // Fall back to any enabled model
    if (enabledModels.length > 0) {
      return enabledModels[0].model_name;
    }

    throw new Error('No enabled models found');
  }
}
```

**4. Refactor ChatCoordinator (formerly ChatService)**
```typescript
// src/lib/services/chat/ChatCoordinator.ts (NEW - refactored from chat.ts)
export class ChatCoordinator {
  private currentController: AbortController | null = null;
  private streamResponse = true;
  private currentBranchId: string | null = null;
  private lastMessageId: string | null = null;

  // Injected dependencies
  private attachmentProcessor: AttachmentProcessor;
  private apiKeyManager: ApiKeyManager;
  private modelResolver: ModelResolver;

  constructor() {
    this.attachmentProcessor = new AttachmentProcessor();
    this.apiKeyManager = new ApiKeyManager();
    this.modelResolver = new ModelResolver();
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

      // Step 1: Process attachments
      const processedAttachments = await this.attachmentProcessor.processAttachments(
        attachments,
        content
      );

      // Step 2: Prepare content
      let processedContent = this.prepareContent(content, processedAttachments);

      // Step 3: Create message
      const message = this.createMessage(processedContent, processedAttachments);

      // Step 4: Get or create conversation
      const conversation = conversationService.getCurrentConversation()
        ?? await conversationService.setCurrentConversation(null);
      const history = await conversationService.getAPIHistory(conversation.id);

      // Step 5: Resolve model
      const selectedModel = await this.modelResolver.resolveModel(model);

      // Step 6: Get API key
      const apiKey = await this.apiKeyManager.getApiKey(selectedModel.provider);

      // Step 7: Get provider and send request
      const provider = providerRegistry.get(selectedModel.provider);
      const aiResponse = await provider.createChatCompletion(
        selectedModel.model_name,
        await formatMessages(history, message, systemPrompt || "You are a helpful AI assistant."),
        this.streamResponse,
        onStreamResponse,
        this.currentController.signal
      );

      this.currentController = null;

      // Extract response content and usage
      const modelResponse = typeof aiResponse === 'string' ? aiResponse : aiResponse.content;
      const usage = typeof aiResponse === 'object' ? aiResponse.usage : undefined;

      // Step 8: Save messages and track usage
      await this.saveMessagesAndTrackUsage(
        conversation.id,
        message,
        modelResponse,
        selectedModel,
        usage,
        userMessageId,
        assistantMessageId
      );

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

  private prepareContent(
    content: string,
    attachments: Attachment[]
  ): string {
    const audioTranscripts = attachments
      .filter(att => att.attachment_type.startsWith("audio") && att.transcript)
      .map(att => `[Audio Transcript]: ${att.transcript}`);

    if (audioTranscripts.length > 0) {
      return content + '\n' + audioTranscripts.join('\n');
    }
    return content;
  }

  private async saveMessagesAndTrackUsage(
    conversationId: string,
    message: DisplayMessage,
    modelResponse: string,
    selectedModel: Model,
    usage: any,
    userMessageId?: string,
    assistantMessageId?: string
  ): Promise<void> {
    // Get or create main branch
    if (!this.currentBranchId) {
      const mainBranch = await branchService.getOrCreateMainBranch(conversationId);
      this.currentBranchId = mainBranch.id;
    }

    // Save messages
    const [savedUserMessageId, savedAssistantMessageId] = await Promise.all([
      conversationService.saveMessage('user', message.content, message.attachments || [], undefined, userMessageId),
      conversationService.saveMessage('assistant', modelResponse, [], undefined, assistantMessageId)
    ]);

    // Create tree nodes
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

    // Track usage in background
    if (usage && savedAssistantMessageId) {
      this.trackUsageInBackground(
        conversationId,
        savedAssistantMessageId,
        selectedModel,
        usage
      );
    }
  }

  private trackUsageInBackground(
    conversationId: string,
    messageId: string,
    model: Model,
    usage: any
  ): void {
    Promise.resolve().then(async () => {
      try {
        const cost = calculateCost(
          model.model_name,
          usage.prompt_tokens,
          usage.completion_tokens
        );

        const [, updatedUsage] = await Promise.all([
          invoke('save_message_usage', {
            input: {
              message_id: messageId,
              model_name: model.model_name,
              prompt_tokens: usage.prompt_tokens,
              completion_tokens: usage.completion_tokens,
              total_tokens: usage.prompt_tokens + usage.completion_tokens,
              estimated_cost: cost
            }
          }),
          invoke<ConversationUsageSummary>('update_conversation_usage', {
            conversationId
          })
        ]);

        currentConversationUsage.set(updatedUsage);
      } catch (usageError) {
        console.warn('Failed to save usage data:', usageError);
      }
    });
  }

  // ... other methods (cancelCurrentRequest, initializeBranchContext, etc.)
}
```

**Files to Create:**
- `src/lib/services/chat/ChatCoordinator.ts`
- `src/lib/services/chat/AttachmentProcessor.ts`
- `src/lib/services/chat/ApiKeyManager.ts`
- `src/lib/services/chat/ModelResolver.ts`
- `src/lib/services/chat/index.ts`

**Files to Modify:**
- `src/lib/services/chat.ts` - DELETE (replaced by ChatCoordinator)
- Update all imports from `./chat` to `./chat/ChatCoordinator`

**Success Criteria:**
- ✅ No single service over 200 lines
- ✅ Each service has single responsibility
- ✅ All tests pass
- ✅ No regression in functionality

---

### Phase 4: State Management Consolidation (Days 8-10)

**Objective:** Consolidate scattered stores and remove anti-patterns.

**Current Problem:**
- ConversationService has internal writable state (anti-pattern)
- Multiple stores for related data
- No single source of truth
- State updates are scattered

**Solution:**
```typescript
// src/lib/stores/conversation/types.ts (NEW)
export interface ConversationState {
  currentConversationId: string | null;
  currentConversation: Conversation | null;
  messages: DisplayMessage[];
  isLoading: boolean;
  error: string | null;
}

export interface ChatState {
  availableModels: Model[];
  selectedModel: string;
  selectedSystemPrompt: SystemPrompt | null;
  systemPrompts: SystemPrompt[];
  streamingEnabled: boolean;
  attachments: Attachment[];
  currentMessage: string;
  isFirstMessage: boolean;
  isStreaming: boolean;
  streamingMessage: string;
}
```

```typescript
// src/lib/stores/conversation/store.ts (NEW - refactored)
import { writable, derived, get } from 'svelte/store';
import type { ConversationState } from './types';

function createConversationStore() {
  const { subscribe, set, update } = writable<ConversationState>({
    currentConversationId: null,
    currentConversation: null,
    messages: [],
    isLoading: false,
    error: null
  });

  return {
    subscribe,

    // Actions
    setConversation: async (conversationId: string | null) => {
      update(state => ({ ...state, isLoading: true, error: null }));
      try {
        const conversation = await invoke<Conversation>('get_or_create_conversation', { conversationId });
        const messages = await invoke<DBMessage[]>('get_conversation_history', { conversationId: conversation.id });

        update(state => ({
          ...state,
          currentConversationId: conversation.id,
          currentConversation: conversation,
          messages: messages.map(toDisplayMessage),
          isLoading: false
        }));
      } catch (error) {
        update(state => ({
          ...state,
          isLoading: false,
          error: error instanceof Error ? error.message : 'Unknown error'
        }));
      }
    },

    addMessage: (message: DisplayMessage) => {
      update(state => ({
        ...state,
        messages: [...state.messages, message]
      }));
    },

    clearMessages: () => {
      update(state => ({
        ...state,
        messages: [],
        currentConversationId: null,
        currentConversation: null
      }));
    },

    // ... other actions
  };
}

export const conversationStore = createConversationStore();

// Derived stores for specific use cases
export const currentConversation = derived(
  conversationStore,
  $state => $state.currentConversation
);

export const currentMessages = derived(
  conversationStore,
  $state => $state.messages
);
```

```typescript
// src/lib/services/conversation.ts - REFACTORED
import { conversationStore } from '$lib/stores/conversation/store';
import { invoke } from '@tauri-apps/api/tauri';

/**
 * Service for conversation operations
 * NO INTERNAL STATE - uses conversationStore
 */
export class ConversationService {
  async setCurrentConversation(conversationId: string | null): Promise<Conversation> {
    await conversationStore.setConversation(conversationId);
    const state = get(conversationStore);
    return state.currentConversation!;
  }

  getCurrentConversation(): Conversation | null {
    const state = get(conversationStore);
    return state.currentConversation;
  }

  async getDisplayHistory(conversationId: string): Promise<DisplayMessage[]> {
    const history = await invoke<DBMessage[]>('get_conversation_history', { conversationId });
    return history
      .sort((a, b) => (a.timestamp ?? 0) - (b.timestamp ?? 0))
      .map(toDisplayMessage);
  }

  async getAPIHistory(conversationId: string): Promise<APIMessage[]> {
    const history = await invoke<DBMessage[]>('get_conversation_history', { conversationId });
    return history
      .sort((a, b) => (a.timestamp ?? 0) - (b.timestamp ?? 0))
      .map(toAPIMessage);
  }

  // ... other methods WITHOUT internal state
}
```

**Files to Create:**
- `src/lib/stores/conversation/types.ts`
- `src/lib/stores/conversation/store.ts`
- `src/lib/stores/conversation/index.ts`

**Files to Modify:**
- `src/lib/services/conversation.ts` - Remove internal state
- `src/lib/components/chat/store.ts` - Use conversationStore
- All components using conversation state

**Success Criteria:**
- ✅ No services with internal writable stores
- ✅ Single source of truth for conversation state
- ✅ Clear data flow: Store → Service → UI
- ✅ All reactive updates work correctly

---

### Phase 5: Code Organization & Cleanup (Days 11-12)

**Objective:** Extract constants, improve error handling, remove duplication.

**Tasks:**

**1. Extract Configuration**
```typescript
// src/lib/config/constants.ts (NEW)
export const API_CONFIG = {
  DEFAULT_SYSTEM_PROMPT: 'You are a helpful AI assistant.',
  MAX_RETRY_ATTEMPTS: 3,
  RETRY_DELAY_MS: 1000,
  REQUEST_TIMEOUT_MS: 120000,
} as const;

export const PROVIDER_NAMES = {
  OPENAI: 'openai',
  ANTHROPIC: 'anthropic',
  DEEPSEEK: 'deepseek',
  CUSTOM: 'custom',
} as const;

export const MESSAGE_TYPES = {
  USER: 'user',
  ASSISTANT: 'assistant',
  SYSTEM: 'system',
} as const;
```

**2. Add Error Boundaries**
```typescript
// src/lib/utils/errors.ts (NEW)
export class AIServiceError extends Error {
  constructor(
    message: string,
    public readonly provider: string,
    public readonly originalError?: unknown
  ) {
    super(message);
    this.name = 'AIServiceError';
  }
}

export class ModelNotFoundError extends Error {
  constructor(public readonly modelName: string) {
    super(`Model not found: ${modelName}`);
    this.name = 'ModelNotFoundError';
  }
}

export class ApiKeyError extends Error {
  constructor(public readonly provider: string) {
    super(`API key not found for provider: ${provider}`);
    this.name = 'ApiKeyError';
  }
}

// Error handler utility
export function handleServiceError(error: unknown): never {
  if (error instanceof AIServiceError) {
    console.error(`[${error.provider}] ${error.message}`, error.originalError);
  } else if (error instanceof Error) {
    console.error(error.message, error);
  } else {
    console.error('Unknown error:', error);
  }
  throw error;
}
```

**3. Remove Hardcoded Values**
Replace all instances of:
- Hard-coded system prompts with `API_CONFIG.DEFAULT_SYSTEM_PROMPT`
- Magic numbers with named constants
- String literals with constants from `PROVIDER_NAMES`, etc.

**4. Add JSDoc Comments**
Add comprehensive documentation to all public APIs:
```typescript
/**
 * Coordinates the chat message flow from user input to AI response
 *
 * @example
 * ```typescript
 * const coordinator = new ChatCoordinator();
 * const result = await coordinator.handleSendMessage(
 *   'Hello',
 *   'gpt-4',
 *   (chunk) => console.log(chunk)
 * );
 * ```
 */
export class ChatCoordinator {
  /**
   * Sends a message to the AI and handles the response
   *
   * @param content - The user's message content
   * @param model - The model name to use (e.g., 'gpt-4')
   * @param onStreamResponse - Callback for streaming responses
   * @param systemPrompt - Optional system prompt override
   * @param attachments - Optional file/image attachments
   * @param userMessageId - Optional ID for idempotency
   * @param assistantMessageId - Optional ID for idempotency
   * @returns Promise with response text and conversation ID
   * @throws {ModelNotFoundError} If model doesn't exist
   * @throws {ApiKeyError} If API key is missing
   * @throws {AIServiceError} If AI service fails
   */
  async handleSendMessage(
    content: string,
    model: string,
    onStreamResponse: (chunk: string) => void,
    systemPrompt?: string,
    attachments: Attachment[] = [],
    userMessageId?: string,
    assistantMessageId?: string,
  ): Promise<{ text: string; conversationId: string } | undefined> {
    // ...
  }
}
```

**Files to Create:**
- `src/lib/config/constants.ts`
- `src/lib/utils/errors.ts`

**Files to Modify:**
- All services - replace hardcoded values
- All services - add JSDoc comments
- All services - use custom error classes

**Success Criteria:**
- ✅ No magic numbers or hardcoded strings
- ✅ All public APIs documented
- ✅ Consistent error handling
- ✅ Better developer experience

---

## Migration Strategy

### Incremental Approach
1. **Branch per Phase**: Create feature branch for each phase
2. **Review & Test**: Get approval before merging
3. **Small PRs**: Keep changes focused and reviewable
4. **Backward Compatibility**: Maintain old exports until migration complete

### Example Migration (Phase 2)
```typescript
// OLD (chat.ts)
export const chatService = new ChatService();

// NEW (chat/index.ts)
export const chatCoordinator = new ChatCoordinator();

// COMPATIBILITY LAYER (temporary)
export const chatService = chatCoordinator; // Same interface!
```

### Rollback Plan
- Each phase is independently reversible
- Git branches allow easy rollback
- Feature flags for gradual rollout (if needed)

---

## Testing Approach

### Test Coverage Strategy
1. **Unit Tests**: Each extracted service
2. **Integration Tests**: Provider registry, message flow
3. **Type Tests**: Compile-time type checking
4. **Manual Tests**: Full user flows

### Example Test
```typescript
// src/lib/services/chat/AttachmentProcessor.test.ts
import { describe, it, expect, vi } from 'vitest';
import { AttachmentProcessor } from './AttachmentProcessor';

describe('AttachmentProcessor', () => {
  it('should process audio attachments', async () => {
    const processor = new AttachmentProcessor();
    const attachments = [{
      attachment_type: 'audio',
      data: 'base64data',
      name: 'test.mp3',
      transcript: undefined
    }];

    const result = await processor.processAttachments(attachments, 'test');

    expect(result[0].transcript).toBeDefined();
    expect(result[0].transcript).not.toBe('[Transcription failed]');
  });

  it('should handle transcription failures gracefully', async () => {
    // Mock API key to be missing
    vi.mock('@tauri-apps/api/tauri', () => ({
      invoke: vi.fn().mockResolvedValue(null)
    }));

    const processor = new AttachmentProcessor();
    const attachments = [{
      attachment_type: 'audio',
      data: 'base64data',
      name: 'test.mp3',
      transcript: undefined
    }];

    const result = await processor.processAttachments(attachments, 'test');

    expect(result[0].transcript).toBe('[Transcription failed]');
  });
});
```

---

## Success Criteria

### Code Quality Metrics
- ✅ No file over 300 lines
- ✅ No function over 50 lines
- ✅ No `any` types (except necessary FFI boundaries)
- ✅ 80%+ test coverage on new code
- ✅ Zero TypeScript errors
- ✅ Zero linter errors

### Functionality
- ✅ All existing features work identically
- ✅ No performance regression
- ✅ No memory leaks introduced
- ✅ Backward compatible (until migration complete)

### Maintainability
- ✅ Clear separation of concerns
- ✅ Easy to understand for new developers
- ✅ Well-documented public APIs
- ✅ Consistent patterns throughout

---

## Timeline

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Phase 1: Type System | 2 days | Unified type hierarchy |
| Phase 2: Provider Registry | 2 days | Pluggable provider system |
| Phase 3: ChatService Split | 3 days | Focused services |
| Phase 4: State Consolidation | 3 days | Single source of truth |
| Phase 5: Code Cleanup | 2 days | Constants, docs, errors |
| **Total** | **12 days** | **Clean, maintainable codebase** |

---

## Next Steps

After completing all phases, the codebase will be ready for:
1. **Adding MCP Server Support** - Plugin architecture will be in place
2. **Memory Systems** - Clean hooks for memory integration
3. **Workflow Engine** - Clear message pipeline for workflows
4. **Advanced Features** - Extensible foundation for future needs

---

## Notes

- This plan focuses ONLY on refactoring existing code
- No new features will be added during this refactoring
- All changes maintain backward compatibility during transition
- Each phase can be reviewed and approved independently
- Success is measured by code quality, not feature additions

---

**Last Updated:** 2025-01-12
**Status:** Ready for Implementation
