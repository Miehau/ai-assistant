# Complete Examples

## Full Feature Implementation: Conversation Management

This example shows a complete feature implementation following all best practices.

### 1. Model Definition

```rust
// db/models/conversation.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateConversationRequest {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConversationRequest {
    pub name: String,
}
```

### 2. Operations Trait

```rust
// db/operations/conversations.rs
use rusqlite::{params, Result as RusqliteResult};
use chrono::{TimeZone, Utc};
use crate::db::models::Conversation;
use crate::db::error::DatabaseError;
use super::DbOperations;

pub trait ConversationOperations: DbOperations {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at FROM conversations ORDER BY created_at DESC"
        )?;
        
        let conversation_iter = stmt.query_map([], |row| {
            let timestamp: i64 = row.get(2)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            
            Ok(Conversation {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at,
            })
        })?;
        
        conversation_iter.collect()
    }

    fn get_conversation(&self, id: &str) -> Result<Conversation, DatabaseError> {
        let conn = self.conn().lock().unwrap();
        
        conn.query_row(
            "SELECT id, name, created_at FROM conversations WHERE id = ?1",
            params![id],
            |row| {
                let timestamp: i64 = row.get(2)?;
                let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
                
                Ok(Conversation {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at,
                })
            }
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                DatabaseError::ConversationNotFound(id.to_string())
            }
            _ => DatabaseError::Rusqlite(e)
        })
    }

    fn get_or_create_conversation(&self, conversation_id: &str) -> RusqliteResult<Conversation> {
        let conn = self.conn().lock().unwrap();
        
        let mut stmt = conn.prepare("SELECT id, name, created_at FROM conversations WHERE id = ?1")?;
        let existing_conversation = stmt.query_row(params![conversation_id], |row| {
            let timestamp: i64 = row.get(2)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            Ok(Conversation {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at,
            })
        });

        match existing_conversation {
            Ok(conversation) => Ok(conversation),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let created_at = Utc::now();
                let created_at_timestamp = created_at.timestamp();
                conn.execute(
                    "INSERT INTO conversations (id, name, created_at) VALUES (?1, ?2, ?3)",
                    params![conversation_id, "New Conversation", created_at_timestamp],
                )?;

                Ok(Conversation {
                    id: conversation_id.to_string(),
                    name: "New Conversation".to_string(),
                    created_at,
                })
            }
            Err(e) => Err(e),
        }
    }
    
    fn update_conversation_name(&self, conversation_id: &str, name: &str) -> RusqliteResult<()> {
        // Validate name
        if name.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        
        let conn = self.conn().lock().unwrap();
        
        let rows_affected = conn.execute(
            "UPDATE conversations SET name = ?1 WHERE id = ?2",
            params![name, conversation_id],
        )?;
        
        if rows_affected == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        
        Ok(())
    }

    fn delete_conversation(&self, conversation_id: &str) -> RusqliteResult<()> {
        let mut conn = self.conn().lock().unwrap();
        let tx = conn.transaction()?;
        
        // Delete attachments first
        tx.execute(
            "DELETE FROM message_attachments WHERE message_id IN 
             (SELECT id FROM messages WHERE conversation_id = ?1)",
            params![conversation_id],
        )?;
        
        // Delete messages
        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![conversation_id],
        )?;
        
        // Delete conversation
        tx.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![conversation_id],
        )?;
        
        tx.commit()?;
        
        Ok(())
    }
}
```

### 3. Implement Trait on Db

```rust
// db/mod.rs
impl ConversationOperations for Db {}
```

### 4. Command Layer

```rust
// commands/conversations.rs
use crate::db::{Conversation, Db, ConversationOperations};
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    ConversationOperations::get_conversations(&*state)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation(state: State<'_, Db>, id: String) -> Result<Conversation, String> {
    ConversationOperations::get_conversation(&*state, &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_or_create_conversation(
    state: State<'_, Db>,
    conversation_id: Option<String>
) -> Result<Conversation, String> {
    let conversation_id = conversation_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    
    ConversationOperations::get_or_create_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_conversation_name(
    state: State<'_, Db>,
    conversation_id: String,
    name: String
) -> Result<(), String> {
    println!("Updating conversation {} with name: {}", conversation_id, name);
    
    ConversationOperations::update_conversation_name(&*state, &conversation_id, &name)
        .map_err(|e| {
            eprintln!("Error updating conversation name: {}", e);
            e.to_string()
        })
}

#[tauri::command]
pub fn delete_conversation(
    state: State<'_, Db>,
    conversation_id: String
) -> Result<(), String> {
    println!("Deleting conversation: {}", conversation_id);
    
    ConversationOperations::delete_conversation(&*state, &conversation_id)
        .map_err(|e| {
            eprintln!("Error deleting conversation: {}", e);
            e.to_string()
        })
}
```

### 5. Register Commands

```rust
// main.rs
.invoke_handler(tauri::generate_handler![
    commands::get_conversations,
    commands::get_conversation,
    commands::get_or_create_conversation,
    commands::update_conversation_name,
    commands::delete_conversation,
])
```

### 6. Frontend Integration

```typescript
// lib/api/conversations.ts
import { invoke } from '@tauri-apps/api/tauri';

export interface Conversation {
  id: string;
  name: string;
  created_at: string;
}

export async function getConversations(): Promise<Conversation[]> {
  return await invoke('get_conversations');
}

export async function getConversation(id: string): Promise<Conversation> {
  return await invoke('get_conversation', { id });
}

export async function getOrCreateConversation(
  conversationId?: string
): Promise<Conversation> {
  return await invoke('get_or_create_conversation', { 
    conversationId: conversationId ?? null 
  });
}

export async function updateConversationName(
  conversationId: string,
  name: string
): Promise<void> {
  await invoke('update_conversation_name', { conversationId, name });
}

export async function deleteConversation(conversationId: string): Promise<void> {
  await invoke('delete_conversation', { conversationId });
}
```

---

## Example: File Upload with Database Recording

### 1. File Manager State

```rust
// files/mod.rs
use std::fs;
use std::path::PathBuf;

pub struct FileManager {
    base_path: PathBuf,
}

impl FileManager {
    pub fn new() -> Result<Self, std::io::Error> {
        let base_path = dirs::data_dir()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not find data directory"
            ))?
            .join("app_files");
        
        fs::create_dir_all(&base_path)?;
        
        Ok(FileManager { base_path })
    }
    
    pub fn save_file(
        &self,
        conversation_id: &str,
        data: &[u8],
        filename: &str
    ) -> Result<SavedFileInfo, std::io::Error> {
        let file_id = uuid::Uuid::new_v4().to_string();
        let file_path = self.base_path
            .join(conversation_id)
            .join(&file_id);
        
        fs::create_dir_all(file_path.parent().unwrap())?;
        fs::write(&file_path, data)?;
        
        Ok(SavedFileInfo {
            id: file_id,
            filename: filename.to_string(),
            path: file_path.to_string_lossy().to_string(),
            size: data.len(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedFileInfo {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub size: usize,
}
```

### 2. Database Operations

```rust
// db/operations/files.rs
pub trait FileOperations: DbOperations {
    fn record_file(&self, file_info: &SavedFileInfo, conversation_id: &str) 
        -> RusqliteResult<()> {
        let conn = self.conn().lock().unwrap();
        
        conn.execute(
            "INSERT INTO files (id, filename, path, size, conversation_id, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                file_info.id,
                file_info.filename,
                file_info.path,
                file_info.size as i64,
                conversation_id,
                Utc::now().timestamp()
            ],
        )?;
        
        Ok(())
    }
}
```

### 3. Command with Multiple State

```rust
// commands/files.rs
#[tauri::command]
pub fn upload_file(
    db: State<'_, Db>,
    file_manager: State<'_, FileManager>,
    conversation_id: String,
    file_data: Vec<u8>,
    filename: String,
) -> Result<SavedFileInfo, String> {
    // Save file to disk
    let file_info = file_manager
        .save_file(&conversation_id, &file_data, &filename)
        .map_err(|e| format!("Failed to save file: {}", e))?;
    
    // Record in database
    FileOperations::record_file(&*db, &file_info, &conversation_id)
        .map_err(|e| format!("Failed to record file: {}", e))?;
    
    Ok(file_info)
}
```

---

## Example: Migration

```rust
// db/mod.rs
use rusqlite_migration::{Migrations, M};

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            "CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )"
        ),
        M::up(
            "CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            )"
        ),
        M::up(
            "CREATE INDEX idx_messages_conversation 
             ON messages(conversation_id, created_at)"
        ),
        M::up(
            "ALTER TABLE conversations ADD COLUMN updated_at INTEGER"
        ),
    ])
}

impl Db {
    pub fn run_migrations(&mut self) -> Result<(), rusqlite_migration::Error> {
        let mut conn = self.conn.lock().unwrap();
        migrations().to_latest(&mut conn)?;
        Ok(())
    }
}
```

---

## Refactoring Example: Before and After

### Before (Anti-pattern)

```rust
// ❌ All logic in command
#[tauri::command]
pub fn save_message(
    state: State<'_, Db>,
    conversation_id: String,
    role: String,
    content: String,
) -> Result<String, String> {
    let conn = state.conn.lock().unwrap();
    let message_id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp();
    
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) 
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![message_id, conversation_id, role, content, created_at],
    ).map_err(|e| e.to_string())?;
    
    // Update conversation timestamp
    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        params![created_at, conversation_id],
    ).map_err(|e| e.to_string())?;
    
    Ok(message_id)
}
```

### After (Best Practice)

```rust
// ✅ Clean separation of concerns

// Operation trait
pub trait MessageOperations: DbOperations {
    fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
    ) -> RusqliteResult<String> {
        let mut conn = self.conn().lock().unwrap();
        let tx = conn.transaction()?;
        
        let message_id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();
        
        // Insert message
        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&message_id, conversation_id, role, content, created_at],
        )?;
        
        // Update conversation
        tx.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![created_at, conversation_id],
        )?;
        
        tx.commit()?;
        
        Ok(message_id)
    }
}

// Command (thin wrapper)
#[tauri::command]
pub fn save_message(
    state: State<'_, Db>,
    conversation_id: String,
    role: String,
    content: String,
) -> Result<String, String> {
    MessageOperations::save_message(&*state, &conversation_id, &role, &content)
        .map_err(|e| e.to_string())
}
```

---

## Testing Example

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_db() -> Db {
        let db = Db::new(":memory:").unwrap();
        db.run_migrations().unwrap();
        db
    }
    
    #[test]
    fn test_create_and_get_conversation() {
        let db = create_test_db();
        
        // Create
        let result = ConversationOperations::get_or_create_conversation(&db, "test-id");
        assert!(result.is_ok());
        
        let conversation = result.unwrap();
        assert_eq!(conversation.id, "test-id");
        assert_eq!(conversation.name, "New Conversation");
        
        // Get
        let result = ConversationOperations::get_conversation(&db, "test-id");
        assert!(result.is_ok());
        
        let fetched = result.unwrap();
        assert_eq!(fetched.id, conversation.id);
    }
    
    #[test]
    fn test_update_conversation_name() {
        let db = create_test_db();
        
        ConversationOperations::get_or_create_conversation(&db, "test-id").unwrap();
        
        let result = ConversationOperations::update_conversation_name(
            &db,
            "test-id",
            "Updated Name"
        );
        assert!(result.is_ok());
        
        let conversation = ConversationOperations::get_conversation(&db, "test-id").unwrap();
        assert_eq!(conversation.name, "Updated Name");
    }
    
    #[test]
    fn test_delete_conversation() {
        let db = create_test_db();
        
        ConversationOperations::get_or_create_conversation(&db, "test-id").unwrap();
        
        let result = ConversationOperations::delete_conversation(&db, "test-id");
        assert!(result.is_ok());
        
        let result = ConversationOperations::get_conversation(&db, "test-id");
        assert!(matches!(result, Err(DatabaseError::ConversationNotFound(_))));
    }
}
```
