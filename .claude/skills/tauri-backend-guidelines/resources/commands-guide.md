# Commands Guide

## Overview

Tauri commands are Rust functions exposed to the frontend via IPC. They should be thin wrappers that delegate to operations.

## Command Structure

### Basic Command

```rust
#[tauri::command]
pub fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    ConversationOperations::get_conversations(&*state)
        .map_err(|e| e.to_string())
}
```

### Command with Parameters

```rust
#[tauri::command]
pub fn save_message(
    state: State<'_, Db>,
    conversation_id: String,
    role: String,
    content: String,
    attachments: Vec<IncomingAttachment>,
    message_id: Option<String>,
) -> Result<String, String> {
    MessageOperations::save_message(
        &*state,
        &conversation_id,
        &role,
        &content,
        &attachments,
        message_id
    ).map_err(|e| e.to_string())
}
```

### Command with Multiple State

```rust
#[tauri::command]
pub fn upload_file(
    db: State<'_, Db>,
    file_manager: State<'_, FileManager>,
    conversation_id: String,
    file_data: Vec<u8>,
    filename: String,
) -> Result<FileInfo, String> {
    file_manager.save_file(&conversation_id, &file_data, &filename)
        .map_err(|e| e.to_string())
}
```

## Error Handling Pattern

### Always Convert Errors to String

Commands must return `Result<T, String>` for frontend consumption:

```rust
#[tauri::command]
pub fn my_command(state: State<'_, Db>) -> Result<Data, String> {
    MyOperations::do_something(&*state)
        .map_err(|e| e.to_string())  // Convert custom error to String
}
```

### Optional Debug Logging

```rust
#[tauri::command]
pub fn delete_conversation(state: State<'_, Db>, conversation_id: String) -> Result<(), String> {
    println!("Tauri command delete_conversation called with id={}", conversation_id);
    
    ConversationOperations::delete_conversation(&*state, &conversation_id)
        .map_err(|e| {
            println!("Error in delete_conversation command: {}", e);
            e.to_string()
        })
}
```

## Command Registration

### In main.rs

```rust
.invoke_handler(tauri::generate_handler![
    // Conversation commands
    commands::get_or_create_conversation,
    commands::get_conversations,
    commands::update_conversation_name,
    commands::delete_conversation,
    
    // Message commands
    commands::save_message,
    commands::get_conversation_history,
    
    // File commands
    commands::upload_file,
    commands::delete_file,
    
    // Add new commands here
])
```

### Command Module Structure

```rust
// commands/mod.rs
mod conversations;
mod messages;
mod files;

pub use conversations::*;
pub use messages::*;
pub use files::*;
```

## Parameter Patterns

### Simple Types

```rust
#[tauri::command]
pub fn get_by_id(state: State<'_, Db>, id: String) -> Result<Item, String> {
    // ...
}
```

### Optional Parameters

```rust
#[tauri::command]
pub fn create_item(
    state: State<'_, Db>,
    name: String,
    description: Option<String>,  // Optional
) -> Result<Item, String> {
    // Handle Option
    let desc = description.unwrap_or_else(|| "No description".to_string());
}
```

### Complex Types (Serializable)

```rust
#[derive(Deserialize)]
pub struct CreateItemRequest {
    pub name: String,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[tauri::command]
pub fn create_item(
    state: State<'_, Db>,
    request: CreateItemRequest,
) -> Result<Item, String> {
    // Use request.name, request.tags, etc.
}
```

### Vec Parameters

```rust
#[tauri::command]
pub fn batch_create(
    state: State<'_, Db>,
    items: Vec<String>,
) -> Result<Vec<String>, String> {
    // Process vector
}
```

## Frontend Integration

### Calling from TypeScript/JavaScript

```typescript
import { invoke } from '@tauri-apps/api/tauri';

// Simple command
const conversations = await invoke<Conversation[]>('get_conversations');

// Command with parameters
const message = await invoke<string>('save_message', {
  conversationId: 'uuid',
  role: 'user',
  content: 'Hello',
  attachments: [],
  messageId: null
});

// Error handling
try {
  await invoke('delete_conversation', { conversationId: id });
} catch (error) {
  console.error('Failed to delete:', error);
}
```

## Best Practices

### ✅ DO

- Keep commands thin (< 10 lines)
- Always use `State<'_, T>` for managed state
- Convert all errors to `String` for frontend
- Use descriptive parameter names
- Return strongly typed results
- Register all commands in main.rs

### ❌ DON'T

- Put business logic in commands
- Use `.unwrap()` (handle errors properly)
- Access database directly
- Return internal error types
- Forget to register new commands
- Use mutable state without locking

## Example Command Module

```rust
// commands/conversations.rs
use crate::db::{Conversation, Db, ConversationOperations};
use tauri::State;

#[tauri::command]
pub fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    ConversationOperations::get_conversations(&*state)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_or_create_conversation(
    state: State<'_, Db>,
    conversation_id: Option<String>
) -> Result<Conversation, String> {
    let conversation_id = conversation_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    
    ConversationOperations::get_or_create_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_conversation_name(
    state: State<'_, Db>,
    conversation_id: String,
    name: String
) -> Result<(), String> {
    ConversationOperations::update_conversation_name(&*state, &conversation_id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_conversation(
    state: State<'_, Db>,
    conversation_id: String
) -> Result<(), String> {
    ConversationOperations::delete_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}
```

## Testing Commands

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{MockRuntime, mock_context};

    #[test]
    fn test_command_parameters() {
        // Test parameter parsing and validation
    }
}
```
