# Refactoring Plan

## Overview

This document provides a prioritized roadmap for addressing the issues identified in the architecture review. Tasks are organized by priority and estimated effort.

---

## Priority Levels

| Priority | Criteria | Timeline |
|----------|----------|----------|
| **P0** | Critical bugs, major performance issues | This week |
| **P1** | Important improvements, significant tech debt | This sprint |
| **P2** | Good improvements, moderate impact | Next sprint |
| **P3** | Nice to have, low urgency | Backlog |

---

## P0: Critical (This Week)

### 1. Delete Dead Code
**Effort**: 30 minutes
**Impact**: Cleaner codebase, smaller bundle

```bash
# Delete unused files
rm src/lib/components/RecipeModal.svelte
rm src/lib/components/AddRecipeModal.svelte
rm src/lib/services/qdrant.ts
rm src/lib/services/LangfuseService.ts
rm src/lib/services/api.ts
rm src/lib/models/apiKeyService.ts
rm -rf src/routes/cv
rm src/lib/types/index.ts

# Remove unused dependencies
bun remove pouchdb @types/pouchdb events @qdrant/js-client-rest langfuse svelte-ux

# Verify build
bun run build
```

---

### 2. Fix N+1 Query in Messages
**Effort**: 1 hour
**Impact**: Major performance improvement for conversations with many messages

**File**: `src-tauri/src/db/operations/messages.rs:165-173`

```rust
// Before: O(M × N) nested iteration
for attachment in attachments {
    if let Some(message) = messages.iter_mut().find(|m| m.id == *message_id) {
        message.attachments.push(att);
    }
}

// After: O(M + N) with HashMap
use std::collections::HashMap;

let mut message_map: HashMap<String, &mut Message> = messages
    .iter_mut()
    .map(|m| (m.id.clone(), m))
    .collect();

for attachment in attachments.flatten() {
    if let Some(message_id) = &attachment.message_id {
        if let Some(message) = message_map.get_mut(message_id) {
            message.attachments.push(attachment);
        }
    }
}
```

---

### 3. Fix N+1 Query in Usage Statistics
**Effort**: 1 hour
**Impact**: Faster usage calculation

**File**: `src-tauri/src/db/operations/usage.rs:46-80`

```rust
// Before: N+1 queries
for message_id in message_ids {
    let usage = conn.query_row(...)?;  // Query per message
}

// After: Single JOIN query
let stats = conn.query_row(
    "SELECT
        COALESCE(SUM(mu.prompt_tokens), 0),
        COALESCE(SUM(mu.completion_tokens), 0),
        COALESCE(SUM(mu.estimated_cost), 0.0),
        COUNT(DISTINCT m.id)
     FROM messages m
     LEFT JOIN message_usage mu ON m.id = mu.message_id
     WHERE m.conversation_id = ?1",
    params![conversation_id],
    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
)?;
```

---

### 4. Remove Duplicate Command Registration
**Effort**: 15 minutes
**Impact**: Cleaner code, less confusion

**File**: `src-tauri/src/main.rs`

Remove duplicate registrations:
- `get_file` (keep line 62, remove line 36)
- `delete_file` (keep line 63, remove line 39)
- `get_image_thumbnail` (keep line 66, remove line 37)
- `optimize_image` (keep line 67, remove line 38)

---

## P1: Important (This Sprint)

### 5. Add Type Generation
**Effort**: 2-3 hours
**Impact**: Eliminates manual type duplication, prevents type drift

**Step 1**: Add dependencies
```toml
# src-tauri/Cargo.toml
[dependencies]
specta = "1.0"
tauri-specta = "1.0"
```

**Step 2**: Annotate types
```rust
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
pub struct Conversation { ... }

#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
pub struct Message { ... }
// ... all model types
```

**Step 3**: Generate types in main.rs
```rust
use tauri_specta::ts;

fn main() {
    #[cfg(debug_assertions)]
    ts::export(
        collect_types![/* all types */],
        "../src/lib/generated/tauri-types.ts"
    ).unwrap();

    // ... rest of setup
}
```

**Step 4**: Update imports
```typescript
// Before
import type { Conversation } from '$lib/types';

// After
import type { Conversation } from '$lib/generated/tauri-types';
```

---

### 6. Create Backend Client
**Effort**: 3-4 hours
**Impact**: Centralized Tauri calls, caching, consistent error handling

**Create**: `src/lib/backend/client.ts`

```typescript
import { invoke } from '@tauri-apps/api/tauri';
import type { Model, Conversation, Message, CustomBackend } from '$lib/generated/tauri-types';

class BackendClient {
  private cache = new Map<string, { data: unknown; timestamp: number }>();

  // Models
  async getModels(): Promise<Model[]> {
    return this.cachedInvoke('get_models', {}, 60000);
  }

  async addModel(model: Omit<Model, 'id'>): Promise<Model> {
    this.invalidateCache('get_models');
    return invoke('add_model', { model });
  }

  // Conversations
  async getConversations(): Promise<Conversation[]> {
    return invoke('get_conversations', {});
  }

  async getOrCreateConversation(id: string | null): Promise<Conversation> {
    return invoke('get_or_create_conversation', { conversationId: id });
  }

  // API Keys
  async getApiKey(provider: string): Promise<string | null> {
    return invoke('get_api_key', { provider });
  }

  async setApiKey(provider: string, apiKey: string): Promise<void> {
    return invoke('set_api_key', { provider, apiKey });
  }

  // Custom Backends
  async getCustomBackends(): Promise<CustomBackend[]> {
    return this.cachedInvoke('get_custom_backends', {}, 60000);
  }

  // Caching helpers
  private async cachedInvoke<T>(
    cmd: string,
    args: object,
    ttl: number
  ): Promise<T> {
    const key = `${cmd}:${JSON.stringify(args)}`;
    const cached = this.cache.get(key);

    if (cached && Date.now() - cached.timestamp < ttl) {
      return cached.data as T;
    }

    const data = await invoke<T>(cmd, args);
    this.cache.set(key, { data, timestamp: Date.now() });
    return data;
  }

  private invalidateCache(prefix: string): void {
    for (const key of this.cache.keys()) {
      if (key.startsWith(prefix)) {
        this.cache.delete(key);
      }
    }
  }
}

export const backend = new BackendClient();
```

**Migrate services** to use `backend` instead of direct `invoke()`.

---

### 7. Move Store to Correct Location
**Effort**: 1 hour
**Impact**: Better code organization

```bash
# Move file
mv src/lib/components/chat/store.ts src/lib/stores/chat.ts

# Update all imports
# Find: from '$lib/components/chat/store'
# Replace: from '$lib/stores/chat'
```

---

### 8. Fix File I/O Under Lock
**Effort**: 2 hours
**Impact**: Prevents database blocking during file reads

**File**: `src-tauri/src/db/operations/messages.rs`

```rust
// Step 1: Query metadata only (fast, under lock)
struct AttachmentMeta {
    id: String,
    message_id: String,
    name: String,
    attachment_type: String,
    file_path: Option<String>,
    // ... other non-data fields
}

let metadata: Vec<AttachmentMeta> = {
    let conn = self.conn().lock().unwrap();
    // Query without reading file contents
    conn.prepare("SELECT id, message_id, name, attachment_type, file_path FROM attachments WHERE ...")?
        .query_map(params![...], |row| Ok(AttachmentMeta { ... }))?
        .collect()?
};
// Lock released here

// Step 2: Read files (slow, without lock)
let attachments: Vec<MessageAttachment> = metadata
    .into_iter()
    .map(|meta| {
        let data = if let Some(path) = meta.file_path {
            let content = fs::read(&attachments_dir.join(&path))?;
            base64::encode(&content)
        } else {
            String::new()
        };
        Ok(MessageAttachment { data, ...meta })
    })
    .collect::<Result<Vec<_>, _>>()?;
```

---

## P2: Improvements (Next Sprint)

### 9. Split ChatService
**Effort**: 4-6 hours
**Impact**: Better maintainability, testability

**Target structure**:
```
src/lib/services/
├── chat/
│   ├── MessageCoordinator.ts   # Main orchestration (from handleSendMessage)
│   ├── AudioProcessor.ts       # Audio transcription (from processAttachments)
│   └── StreamController.ts     # Abort handling
├── llm/
│   └── factory.ts              # createLLMService()
└── usage/
    └── UsageTracker.ts         # Token/cost tracking
```

**MessageCoordinator.ts**:
```typescript
export class MessageCoordinator {
  constructor(
    private backend: BackendClient,
    private llmFactory: LLMServiceFactory,
    private audioProcessor: AudioProcessor,
    private usageTracker: UsageTracker
  ) {}

  async sendMessage(params: SendMessageParams): Promise<SendMessageResult> {
    // Orchestration logic from chat.ts:211-351
  }
}
```

---

### 10. Extract Business Logic from Store
**Effort**: 3-4 hours
**Impact**: Cleaner separation of concerns

**Before** (`stores/chat.ts`):
```typescript
export async function loadModels() {
  isLoading.set(true);
  try {
    // 85 lines of business logic
  } finally {
    isLoading.set(false);
  }
}
```

**After**:
```typescript
// services/modelLoader.ts
export class ModelLoader {
  async loadModels(): Promise<Model[]> {
    // Business logic here
  }
}

// stores/chat.ts
export const availableModels = writable<Model[]>([]);
export const isLoading = writable(false);

// Usage in component
const loader = new ModelLoader();
isLoading.set(true);
availableModels.set(await loader.loadModels());
isLoading.set(false);
```

---

### 11. Extract Streaming Logic
**Effort**: 2 hours
**Impact**: Removes ~150 lines of duplication

**Create**: `src/lib/services/llm/streaming.ts`

```typescript
export interface StreamParser {
  parse(data: string): string | null;
  isDone(data: string): boolean;
}

export const openAIParser: StreamParser = {
  parse(data) {
    const json = JSON.parse(data);
    return json.choices?.[0]?.delta?.content ?? null;
  },
  isDone(data) {
    return data === '[DONE]';
  }
};

export async function processSSEStream(
  response: Response,
  parser: StreamParser,
  onChunk: (content: string) => void
): Promise<string> {
  const reader = response.body?.getReader();
  const decoder = new TextDecoder();
  let fullResponse = '';

  while (reader) {
    const { done, value } = await reader.read();
    if (done) break;

    const lines = decoder.decode(value).split('\n');
    for (const line of lines) {
      if (!line.startsWith('data: ')) continue;

      const data = line.slice(6);
      if (parser.isDone(data)) continue;

      const content = parser.parse(data);
      if (content) {
        fullResponse += content;
        onChunk(content);
      }
    }
  }

  return fullResponse;
}
```

---

### 12. Standardize Error Handling
**Effort**: 2-3 hours
**Impact**: Consistent error handling across services

**Create**: `src/lib/services/base/ServiceError.ts`

```typescript
export class ServiceError extends Error {
  constructor(
    public operation: string,
    public service: string,
    public cause?: unknown,
    public recoverable: boolean = false
  ) {
    super(`[${service}] ${operation} failed`);
    this.name = 'ServiceError';
  }

  static from(operation: string, service: string, error: unknown): ServiceError {
    if (error instanceof ServiceError) return error;
    return new ServiceError(operation, service, error);
  }
}

// Usage
try {
  await this.sendMessage(params);
} catch (error) {
  throw ServiceError.from('sendMessage', 'ChatService', error);
}
```

---

## P3: Nice to Have (Backlog)

### 13. Complete LLM Interface Migration
**Effort**: 4-6 hours
**Impact**: Remove deprecated code, better type safety

1. Update `completion()` to support streaming
2. Migrate ChatService to use new interface
3. Remove all `@deprecated` methods
4. Remove `(llmService as any)` casts

---

### 14. Implement Provider Registry
**Effort**: 2 hours
**Impact**: Easier to add new providers

```typescript
// src/lib/services/llm/registry.ts
type ProviderFactory = (apiKey: string) => LLMService;

const providers = new Map<string, ProviderFactory>();

export function registerProvider(name: string, factory: ProviderFactory) {
  providers.set(name.toLowerCase(), factory);
}

export function createLLMService(provider: string, apiKey: string): LLMService {
  const factory = providers.get(provider.toLowerCase());
  if (!factory) throw new Error(`Unknown provider: ${provider}`);
  return factory(apiKey);
}

// Self-registration in each provider file
registerProvider('openai', (key) => new OpenAIService(key));
```

---

### 15. Add Proper Logging
**Effort**: 2 hours
**Impact**: Better debugging, production monitoring

**Rust**:
```toml
# Cargo.toml
[dependencies]
log = "0.4"
env_logger = "0.10"
```

```rust
use log::{debug, info, warn, error};

// Replace println! with proper logging
debug!("Processing message: {}", message_id);
info!("Conversation created: {}", conversation_id);
warn!("Failed to create branch: {}", error);
error!("Database error: {}", error);
```

**TypeScript**:
```typescript
// src/lib/utils/logger.ts
export const logger = {
  debug: (msg: string, ...args: unknown[]) => {
    if (import.meta.env.DEV) console.debug(`[DEBUG] ${msg}`, ...args);
  },
  info: (msg: string, ...args: unknown[]) => console.info(`[INFO] ${msg}`, ...args),
  warn: (msg: string, ...args: unknown[]) => console.warn(`[WARN] ${msg}`, ...args),
  error: (msg: string, ...args: unknown[]) => console.error(`[ERROR] ${msg}`, ...args),
};
```

---

## Timeline Summary

| Week | Tasks | Effort |
|------|-------|--------|
| Week 1 | P0 tasks (1-4) | ~3 hours |
| Week 2 | P1 tasks (5-8) | ~10 hours |
| Week 3-4 | P2 tasks (9-12) | ~12 hours |
| Backlog | P3 tasks (13-15) | ~10 hours |

**Total estimated effort**: ~35 hours

---

## Success Metrics

After completing P0-P2:

| Metric | Before | After |
|--------|--------|-------|
| Dead code lines | ~650 | 0 |
| Unused dependencies | 6 | 0 |
| N+1 queries | 2 | 0 |
| ChatService lines | 450+ | ~150 |
| Manual type duplication | ~20 types | 0 |
| `invoke()` call locations | 15+ | 1 (backend client) |
| `(as any)` casts | 27 | 0 |
