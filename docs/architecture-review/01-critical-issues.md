# Critical Issues

Issues requiring immediate attention due to performance impact, bugs, or significant technical debt.

## Backend (Rust)

### 1. N+1 Query in Message Retrieval

**Location**: `src-tauri/src/db/operations/messages.rs:165-173`
**Confidence**: 100%

**Problem**: Attachments are matched to messages using nested iteration:

```rust
for attachment in attachments {
    if let Ok(att) = attachment {
        if let Some(message_id) = &att.message_id {
            if let Some(message) = messages.iter_mut().find(|m| m.id == *message_id) {
                message.attachments.push(att);
            }
        }
    }
}
```

**Impact**: O(M × N) operations where M is attachments and N is messages. For 100 messages with 5 attachments each = 500 iterations with linear searches.

**Fix**:
```rust
use std::collections::HashMap;

let mut message_map: HashMap<String, &mut Message> = messages
    .iter_mut()
    .map(|m| (m.id.clone(), m))
    .collect();

for attachment in attachments {
    if let Ok(att) = attachment {
        if let Some(message_id) = &att.message_id {
            if let Some(message) = message_map.get_mut(message_id) {
                message.attachments.push(att);
            }
        }
    }
}
```

---

### 2. N+1 Query in Usage Statistics

**Location**: `src-tauri/src/db/operations/usage.rs:46-80`
**Confidence**: 90%

**Problem**: Fetches all message IDs, then queries usage for each message in a loop.

**Fix**: Use a single JOIN query:
```rust
let (total_prompt_tokens, total_completion_tokens, total_cost, message_count) =
    conn.query_row(
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

### 3. File I/O Under Database Lock

**Location**: `src-tauri/src/db/operations/messages.rs:129-140`
**Confidence**: 95%

**Problem**: File reads occur while holding the database mutex lock:

```rust
let conn = binding.lock().unwrap();
// ... inside query_map callback:
let file_content = fs::read(&full_path)  // FILE I/O WHILE HOLDING DB LOCK!
```

**Impact**: Blocks ALL database operations across the entire application during file reads.

**Fix**: Collect file paths during query, read files after releasing lock:
```rust
// Step 1: Query metadata (fast, under lock)
let attachment_metadata: Vec<AttachmentMeta> = /* query */;
drop(conn);  // Release lock

// Step 2: Read files (slow, without lock)
let attachments = attachment_metadata.into_iter()
    .map(|meta| read_attachment_file(meta))
    .collect();
```

---

### 4. Duplicate Command Registration

**Location**: `src-tauri/src/main.rs:36-67`
**Confidence**: 100%

**Problem**: Several commands are registered twice:
- `commands::get_file` (lines 36 and 62)
- `commands::delete_file` (lines 39 and 63)
- `commands::get_image_thumbnail` (lines 37 and 66)
- `commands::optimize_image` (lines 38 and 67)

**Fix**: Remove duplicates and organize by feature.

---

### 5. Panic-Prone Application Setup

**Location**: `src-tauri/src/main.rs:16-22`
**Confidence**: 100%

**Problem**: Multiple `.expect()` calls will crash the app instead of graceful error handling:

```rust
let app_dir = app.path_resolver().app_data_dir().expect("Failed to get app data dir");
fs::create_dir_all(&app_dir).expect("Failed to create app directory");
let mut db = Db::new(db_path.to_str().unwrap()).expect("Failed to create database");
```

**Fix**: Use proper error propagation with `?` operator.

---

## Frontend (TypeScript)

### 6. ChatService God Class

**Location**: `src/lib/services/chat.ts` (450+ lines)
**Confidence**: 95%

**Problem**: Single class with 10+ responsibilities:
- Message sending
- Model management
- Audio transcription
- Branch context management
- API key retrieval
- Custom backend retrieval
- LLM service factory
- Chat completion orchestration
- Usage tracking
- Stream control

**Fix**: Split into focused services:
- `MessageCoordinator.ts`
- `ModelInfoService.ts`
- `AudioProcessor.ts`
- `LLMServiceFactory.ts`
- `UsageTracker.ts`

---

### 7. No Tauri Invoke Abstraction

**Location**: All service files
**Confidence**: 100%

**Problem**: Direct `invoke()` calls scattered everywhere with no abstraction:
- `get_models` invoked in 4 places
- `get_api_key` invoked in 3 services
- `get_custom_backends` invoked in 2 places

**Impact**:
- Inconsistent error handling
- No centralized caching
- Difficult to mock for testing
- Redundant code

**Fix**: Create unified backend client (see refactoring plan).

---

### 8. Type Safety Bypassed

**Location**: `src/lib/services/chat.ts:362, 433, 446`
**Confidence**: 100%

**Problem**: TypeScript type system bypassed with `any` casts:

```typescript
attachment.transcript = await (llmService as any).transcribeAudio(attachment.data, content);
return (llmService as any).createChatCompletion(...);
```

**Fix**: Complete LLM interface properly, remove all `as any` casts.

---

### 9. Store in Wrong Location

**Location**: `src/lib/components/chat/store.ts` (320 lines)
**Confidence**: 95%

**Problem**: Global application state located in components folder. Contains:
- Messages, models, system prompts (global app state)
- Service orchestration
- Business logic (loadModels, sendMessage functions)

**Fix**: Move to `src/lib/stores/chat.ts` or split into multiple stores.

---

### 10. Manual Type Duplication

**Location**: `src/lib/types/*.ts` ↔ `src-tauri/src/db/models/*.rs`
**Confidence**: 95%

**Problem**: Every type manually maintained in both Rust and TypeScript:
- `Message`, `Conversation`, `Branch`, `Model`, `CustomBackend`, etc.
- No compile-time verification of type alignment
- Types can drift apart silently

**Fix**: Use `tauri-specta` to auto-generate TypeScript types from Rust.

---

## LLM Providers

### 11. Interface Segregation Violation

**Location**: `src/lib/services/base/LLMService.ts:41-44`
**Confidence**: 95%

**Problem**: All providers forced to implement `transcribeAudio()`:

```typescript
abstract transcribeAudio(base64Audio: string, context: string): Promise<string>;
```

Only OpenAI supports this. Other providers throw errors:
- Anthropic: `throw new LLMServiceError('Audio transcription not supported by Claude')`
- DeepSeek: `throw new Error('DeepSeek API error: Not implemented')`
- CustomProvider: `throw new LLMServiceError('Audio transcription not supported')`

**Fix**: Extract audio transcription to optional interface or capability pattern.

---

### 12. Duplicated Streaming Logic

**Location**:
- `src/lib/services/openai.ts:237-287`
- `src/lib/services/deepseek.ts:63-101`
- `src/lib/services/customProvider.ts:106-159`

**Confidence**: 90%

**Problem**: ~50 lines of nearly identical streaming code repeated 3 times:

```typescript
const reader = response.body?.getReader();
const decoder = new TextDecoder();
let fullResponse = '';

while (true) {
  const { done, value } = await reader.read();
  if (done) break;
  const lines = decoder.decode(value).split('\n');
  // ... parsing logic
}
```

**Fix**: Extract to shared utility in base class or separate streaming module.
