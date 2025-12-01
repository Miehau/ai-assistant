# Frontend Services Layer Review

## Overview

The frontend service layer shows signs of organic growth with mixed patterns. The primary issues are a god class (`ChatService`), lack of abstraction over Tauri calls, and state management spread across wrong locations.

---

## Architecture Issues

### 1. ChatService God Class

**File**: `src/lib/services/chat.ts` (450+ lines)

The class has 10+ distinct responsibilities:

| Responsibility | Lines | Should Be |
|---------------|-------|-----------|
| Message sending | 211-351 | MessageCoordinator |
| Model management | 48-118 | ModelInfoService |
| Audio transcription | 353-371, 442-447 | AudioProcessor |
| Branch context | 178-209 | Part of BranchService |
| API key retrieval | 40-46 | BackendClient |
| Custom backend retrieval | 376-383 | BackendClient |
| LLM factory | 21-32 | LLMServiceFactory |
| Chat completion | 385-440 | LLMServiceFactory |
| Usage tracking | 303-337 | UsageTracker |
| Stream control | 165-174 | StreamController |

**Recommended Split**:
```
src/lib/services/
├── chat/
│   ├── MessageCoordinator.ts   # Main orchestration
│   ├── AudioProcessor.ts       # Transcription handling
│   └── StreamController.ts     # Abort/stream management
├── llm/
│   └── factory.ts              # createLLMService()
└── usage/
    └── UsageTracker.ts         # Token/cost tracking
```

---

### 2. No Tauri Invoke Abstraction

**Problem**: Direct `invoke()` calls scattered across services:

```typescript
// chat.ts
const apiKey = await invoke<string | null>('get_api_key', { provider });
const models = await invoke<Model[]>('get_models');

// conversation.ts
const conversation = await invoke<Conversation>('get_or_create_conversation', ...);

// branchService.ts
return await invoke<Branch>('create_branch', ...);
```

**Occurrences**:
| Command | Locations |
|---------|-----------|
| `get_models` | chat.ts:50, chat.ts:97, modelManagement.ts:15, modelService.ts:15 |
| `get_api_key` | chat.ts:41, apiKeyService.ts:21, apiKeyService.ts:46 |
| `get_custom_backends` | chat.ts:68, customBackendService.svelte.ts:21 |

**Fix**: Create unified backend client:

```typescript
// src/lib/backend/client.ts
class BackendClient {
  private cache = new Map<string, { data: unknown; timestamp: number }>();

  async getModels(): Promise<Model[]> {
    return this.cachedInvoke('get_models', {});
  }

  async getApiKey(provider: string): Promise<string | null> {
    return invoke('get_api_key', { provider });
  }

  private async cachedInvoke<T>(cmd: string, args: object, ttl = 60000): Promise<T> {
    const key = `${cmd}:${JSON.stringify(args)}`;
    const cached = this.cache.get(key);
    if (cached && Date.now() - cached.timestamp < ttl) {
      return cached.data as T;
    }
    const data = await invoke<T>(cmd, args);
    this.cache.set(key, { data, timestamp: Date.now() });
    return data;
  }
}

export const backend = new BackendClient();
```

---

### 3. Store in Wrong Location

**File**: `src/lib/components/chat/store.ts` (320 lines)

This file contains global application state but is located in the components folder:
- `messages` store
- `selectedModel` store
- `availableModels` store
- `systemPrompt` store
- `loadModels()` function (85 lines of business logic)
- `sendMessage()` function (108 lines of orchestration)

**Fix**: Move to `src/lib/stores/chat.ts` or split:
```
src/lib/stores/
├── messages.ts
├── models.ts
├── systemPrompts.ts
└── chat.ts  # Composition of above
```

---

### 4. Business Logic in Store Files

**File**: `src/lib/components/chat/store.ts`

Stores should be thin state containers. This file contains:

```typescript
// 85 lines of model loading logic
export async function loadModels() {
  isLoading.set(true);
  try {
    // Complex filtering, merging, API calls...
  } catch (error) {
    // Error handling...
  } finally {
    isLoading.set(false);
  }
}

// 108 lines of message orchestration
export async function sendMessage(content: string, attachments: Attachment[] = []) {
  // Service coordination, error handling, state updates...
}
```

**Fix**: Move business logic to service layer:
```typescript
// src/lib/services/modelLoader.ts
export class ModelLoader {
  async loadModels(): Promise<Model[]> {
    // Business logic here
  }
}

// Store just holds state
export const availableModels = writable<Model[]>([]);
```

---

### 5. State Duplication

**Files**:
- `src/lib/services/conversation.ts:7-10, 21-28`
- `src/lib/components/chat/store.ts`

ConversationService maintains internal state:
```typescript
private state = writable<ConversationState>({
  currentConversationId: null,
  currentConversation: null
});
```

But this state is also implicitly managed through the chat store.

**Fix**: Single source of truth - either service or store, not both.

---

### 6. Inconsistent Error Handling

Three different patterns across services:

**Pattern 1** - Rethrow (chat.ts:343-349):
```typescript
} catch (error: unknown) {
  console.error('Failed to send chat message:', error);
  throw error;
}
```

**Pattern 2** - Return default (titleGenerator.ts:128-131):
```typescript
} catch (error) {
  console.error("Error generating title:", error);
  return "New Conversation";
}
```

**Pattern 3** - Error store (customBackendService.svelte.ts:25-29):
```typescript
} catch (error) {
  this.error = error instanceof Error ? error.message : String(error);
  return [];
}
```

**Fix**: Create unified error handling:
```typescript
// src/lib/services/base/ServiceError.ts
export class ServiceError extends Error {
  constructor(
    public operation: string,
    public cause: unknown,
    public recoverable: boolean = false
  ) {
    super(`${operation} failed: ${cause}`);
  }
}

// Consistent usage
try {
  // ...
} catch (error) {
  throw new ServiceError('sendMessage', error);
}
```

---

### 7. Mixed Service Responsibilities

**File**: `src/lib/services/conversation.ts`

Mixes three concerns:
1. State management (Svelte stores) - lines 7-19
2. Data fetching (invoke calls) - lines 35-97
3. Business operations - lines 99-150

**Fix**: Split into:
```typescript
// ConversationRepository - data access only
export class ConversationRepository {
  async getConversations(): Promise<Conversation[]> { ... }
  async save(conversation: Conversation): Promise<void> { ... }
}

// conversationStore - state management
export const currentConversation = writable<Conversation | null>(null);

// ConversationService - business logic
export class ConversationService {
  constructor(private repo: ConversationRepository) {}
  async switchConversation(id: string): Promise<Conversation> { ... }
}
```

---

## Recommended Service Structure

```
src/lib/
├── backend/              # Tauri abstraction
│   ├── client.ts        # Unified invoke wrapper
│   └── cache.ts         # Response caching
│
├── services/
│   ├── chat/            # Chat feature
│   │   ├── MessageCoordinator.ts
│   │   ├── AudioProcessor.ts
│   │   └── index.ts
│   │
│   ├── conversation/    # Conversation feature
│   │   ├── ConversationService.ts
│   │   ├── BranchService.ts
│   │   └── index.ts
│   │
│   ├── llm/             # LLM providers
│   │   ├── base/
│   │   ├── providers/
│   │   ├── factory.ts
│   │   └── index.ts
│   │
│   └── base/            # Shared utilities
│       ├── ServiceError.ts
│       └── Logger.ts
│
└── stores/              # Pure state management
    ├── messages.ts
    ├── models.ts
    ├── conversations.ts
    └── ui.ts
```

---

## Key Principles

1. **Services don't manipulate stores** - they return data
2. **Single responsibility** - one service, one concern
3. **Dependency injection** - avoid hard-coded singletons
4. **Abstraction layers** - backend client wraps all Tauri invokes
5. **Consistent error handling** - unified error types and patterns

---

## Migration Path

### Phase 1: Create Backend Client
1. Create `src/lib/backend/client.ts`
2. Gradually migrate invoke calls
3. Add caching where beneficial

### Phase 2: Split ChatService
1. Extract `AudioProcessor`
2. Extract `UsageTracker`
3. Create `MessageCoordinator`
4. Remove god class

### Phase 3: Fix State Management
1. Move store to correct location
2. Extract business logic to services
3. Establish single source of truth

### Phase 4: Standardize Patterns
1. Implement unified error handling
2. Add proper logging
3. Document service contracts
