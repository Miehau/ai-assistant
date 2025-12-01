# Types and Data Flow Review

## Overview

The type system has significant issues including manual duplication between Rust and TypeScript, inconsistent timestamp handling, and excessive message transformations. Implementing type generation would eliminate most of these problems.

---

## Type Duplication Problem

### Current State

Every type is manually maintained in both languages:

**Rust** (`src-tauri/src/db/models/custom_backend.rs`):
```rust
pub struct CustomBackend {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_key: Option<String>,
    pub created_at: i64,
}
```

**TypeScript** (`src/lib/types/customBackend.ts`):
```typescript
export interface CustomBackend {
    id: string;
    name: string;
    url: string;
    api_key?: string;
    created_at: number;
}
```

### Duplicated Types

| Type | Rust Location | TypeScript Location |
|------|---------------|---------------------|
| `Message` | `db/models/message.rs` | `types/message.ts` |
| `Conversation` | `db/models/conversation.rs` | `types.ts` |
| `Branch` | `db/models/branch.rs` | `types/branches.ts` |
| `Model` | `db/models/model.rs` | `types/models.ts` |
| `CustomBackend` | `db/models/custom_backend.rs` | `types/customBackend.ts` |
| `SystemPrompt` | `db/models/system_prompt.rs` | `types.ts` |
| `UsageStatistics` | `db/models/usage.rs` | `types.ts` |

### Solution: Type Generation

Add `tauri-specta` to auto-generate TypeScript from Rust:

```toml
# src-tauri/Cargo.toml
[dependencies]
specta = "1.0"
tauri-specta = "1.0"
```

```rust
// Annotate types
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
pub struct CustomBackend {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_key: Option<String>,
    pub created_at: i64,
}
```

```rust
// In main.rs
use tauri_specta::ts;

fn main() {
    ts::export(
        collect_types![/* all command types */],
        "../src/lib/generated/tauri-types.ts"
    ).unwrap();

    // ... rest of setup
}
```

**Benefits**:
- Zero manual type duplication
- Compile-time type safety
- Automatic sync on Rust changes
- Generated invoke wrappers

---

## DateTime Inconsistency

### Current State

Different types use different timestamp representations:

| Type | Field | TypeScript Type | Expected |
|------|-------|-----------------|----------|
| `Conversation` | `created_at` | `string` | ✅ Correct (ISO 8601) |
| `CustomBackend` | `created_at` | `number` | ❌ Wrong (Rust sends string) |
| `Attachment` | `created_at` | `Date` | ❌ Wrong (needs parsing) |
| `Message` | `timestamp` | `number` | ⚠️ Different format |

### Rust Serialization

```rust
// DateTime<Utc> serializes to ISO 8601 string
pub created_at: DateTime<Utc>  // -> "2024-01-15T10:30:00Z"

// i64 stays as number
pub created_at: i64  // -> 1705314600000
```

### Fix: Standardize on Strings

```typescript
// All timestamps as ISO 8601 strings
export interface CustomBackend {
    id: string;
    name: string;
    url: string;
    api_key?: string;
    created_at: string;  // Fix: was number
}

// Utility for parsing
export function parseTimestamp(ts: string | number | Date): Date {
    if (ts instanceof Date) return ts;
    if (typeof ts === 'number') return new Date(ts);
    return new Date(ts);
}

// Utility for display
export function formatTimestamp(ts: string | number | Date): string {
    return parseTimestamp(ts).toLocaleString();
}
```

---

## Attachment Type Complexity

### Current State

Three overlapping attachment types:

**Rust - IncomingAttachment** (frontend → backend):
```rust
pub struct IncomingAttachment {
    pub name: String,
    pub data: String,
    pub attachment_type: String,
    pub description: Option<String>,
    pub transcript: Option<String>,
}
```

**Rust - MessageAttachment** (database):
```rust
pub struct MessageAttachment {
    pub id: Option<String>,
    pub message_id: Option<String>,
    pub name: String,
    pub data: String,
    pub attachment_type: String,
    pub description: Option<String>,
    pub transcript: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub attachment_url: Option<String>,
    pub file_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    pub thumbnail_path: Option<String>,
}
```

**TypeScript - Attachment**:
```typescript
export interface Attachment {
    id?: string;
    message_id?: string;
    name: string;
    data: string;
    attachment_url?: string;
    attachment_type: "image" | "audio" | "text";  // Too restrictive!
    description?: string;
    created_at?: Date;
    transcript?: string;
    file_path?: string;
    file_metadata?: FileMetadata;  // Overlapping fields
}

export interface FileMetadata {
    id: string;
    name: string;
    path: string;
    mime_type: string;
    size_bytes: number;
    created_at: string;
    thumbnail_path?: string;
}
```

### Issues

1. `FileMetadata` duplicates fields from `MessageAttachment`
2. `attachment_type` in TS is restrictive literal union
3. Different field names (`mime_type` vs `file_metadata.mime_type`)

### Fix: Unified Type

```typescript
export interface Attachment {
    // Core (always present)
    name: string;
    data: string;
    attachment_type: string;  // Allow any MIME prefix

    // Database (optional)
    id?: string;
    message_id?: string;
    created_at?: string;
    updated_at?: string;

    // Content (optional)
    description?: string;
    transcript?: string;
    attachment_url?: string;

    // File system (optional)
    file_path?: string;
    size_bytes?: number;
    mime_type?: string;
    thumbnail_path?: string;
}

// Delete FileMetadata interface
```

---

## Message Transformation Chain

### Current Flow

Messages are transformed 3-4 times per request:

```
1. DisplayMessage (UI input)
   ↓
2. APIMessage (history fetch from DB)
   ↓
3. ChatCompletionMessageParam (OpenAI format)
   ↓
4. DBMessage (save to database)
   ↓
5. DisplayMessage (display response)
```

### Code Locations

**Transform 1** - `conversation.ts:40-56`:
```typescript
// DBMessage → DisplayMessage
return history.map(msg => ({
    id: msg.id || uuidv4(),
    type: msg.role === 'user' ? 'sent' : 'received',
    content: msg.content,
    attachments: msg.attachments
}));
```

**Transform 2** - `conversation.ts:59-69`:
```typescript
// DBMessage → APIMessage
return history.map(msg => ({
    role: msg.role,
    content: msg.content
}));
```

**Transform 3** - `messageFormatting.ts:9-17`:
```typescript
// APIMessage → ChatCompletionMessageParam
return [
    { role: 'system', content: systemPrompt },
    ...history.map(formatHistoryMessage),
    await formatUserMessage(currentMessage)
];
```

### Fix: Reduce Transformations

Use `LLMMessage` as canonical internal format:

```typescript
// Single canonical format
import type { LLMMessage } from '$lib/types/llm';

// Transform once from DB
function fromDB(db: DBMessage): LLMMessage {
    return {
        role: db.role as LLMMessageRole,
        content: db.content,
    };
}

// Transform once for UI
function toDisplay(msg: LLMMessage, id: string): DisplayMessage {
    return {
        id,
        type: msg.role === 'user' ? 'sent' : 'received',
        content: typeof msg.content === 'string'
            ? msg.content
            : msg.content[0].text,
    };
}

// Provider-specific transform only at API boundary
function toOpenAI(msg: LLMMessage): ChatCompletionMessageParam {
    // Only OpenAI-specific transformations
}
```

---

## Tauri Command Type Safety

### Current State

No compile-time verification for `invoke()` calls:

```typescript
// conversation.ts
const conversation = await invoke<Conversation>('get_or_create_conversation', { conversationId });

// No verification that:
// - Command name is correct
// - Return type matches Rust
// - Parameters match Rust signature
```

**Rust signature** (`commands/conversations.rs`):
```rust
#[tauri::command]
pub fn get_or_create_conversation(
    state: State<'_, Db>,
    conversation_id: Option<String>  // Note: Option<String>
) -> Result<Conversation, String>
```

### Fix: Generated Command Wrappers

With `tauri-specta`:

```typescript
// Auto-generated src/lib/generated/commands.ts
export async function getOrCreateConversation(
    conversationId: string | null
): Promise<Conversation> {
    return await invoke('get_or_create_conversation', { conversationId });
}

export async function saveMessage(params: {
    conversation_id: string;
    role: string;
    content: string;
    attachments: IncomingAttachment[];
    message_id?: string;
}): Promise<string> {
    return await invoke('save_message', params);
}
```

**Usage**:
```typescript
// Before (unsafe)
const conv = await invoke<Conversation>('get_or_create_conversation', { conversationId });

// After (type-safe)
import { getOrCreateConversation } from '$lib/generated/commands';
const conv = await getOrCreateConversation(conversationId);
```

---

## Model Type Split

### Current State

Two parallel model type systems:

**Database Model** (`types/models.ts`):
```typescript
export interface Model {
    provider: string;
    model_name: string;
    name?: string;
    enabled: boolean;
    url?: string;
    capabilities?: ModelCapabilities;  // From registry
    specs?: ModelSpecs;                // From registry
    custom_backend_id?: string;
}
```

**Registry Model** (`models/registry/types.ts`):
```typescript
export interface ModelConfig {
    id: string;
    name: string;
    provider: string;
    capabilities: ModelCapabilities;
    specs: ModelSpecs;
    defaultParameters: ModelParameters;
}
```

### Fix: Clear Separation

```typescript
// Base from database
export interface DBModel {
    provider: string;
    model_name: string;
    enabled: boolean;
    url?: string;
    deployment_name?: string;
    custom_backend_id?: string;
}

// Enriched with registry data
export interface Model extends DBModel {
    name: string;  // Required after enrichment
    capabilities: ModelCapabilities;
    specs: ModelSpecs;
    defaultParameters?: ModelParameters;
}

// Service ensures all Models have registry data
export class ModelService {
    async getModels(): Promise<Model[]> {
        const dbModels = await backend.getModels();
        return dbModels.map(db => this.enrich(db));
    }

    private enrich(db: DBModel): Model {
        const registry = this.registry.get(db.model_name);
        return {
            ...db,
            name: registry?.name ?? db.model_name,
            capabilities: registry?.capabilities ?? DEFAULT_CAPABILITIES,
            specs: registry?.specs ?? DEFAULT_SPECS,
        };
    }
}
```

---

## `any` Type Usage

### Locations (27 occurrences)

| File | Line | Usage |
|------|------|-------|
| `chat.ts` | 126 | `messages: any[]` |
| `chat.ts` | 387 | `history: any[]` |
| `chat.ts` | 433 | `(llmService as any)` |
| `messageFormatting.ts` | 9 | `history: any[]` |
| `messageFormatting.ts` | 20 | `msg: any` |

### Fix

```typescript
// chat.ts
async generateCompletion(
    messages: LLMMessage[],  // was: any[]
    modelName: string
): Promise<string>

// messageFormatting.ts
export async function formatMessages(
    history: APIMessage[],  // was: any[]
    currentMessage: DisplayMessage,
    systemPrompt?: string
): Promise<ChatCompletionMessageParam[]>
```

---

## Recommendations Summary

### Critical (Implement First)

1. **Add `tauri-specta`** - Eliminates all manual type duplication
2. **Fix DateTime types** - Standardize on ISO 8601 strings
3. **Generate command wrappers** - Type-safe Tauri calls

### Important

4. **Simplify Attachment types** - Remove `FileMetadata`, unify
5. **Reduce transformations** - Use `LLMMessage` as canonical format
6. **Unify Model types** - Clear DB vs enriched distinction

### Nice to Have

7. **Eliminate `any`** - Full type safety
8. **Separate stored vs computed** - Clearer type semantics
