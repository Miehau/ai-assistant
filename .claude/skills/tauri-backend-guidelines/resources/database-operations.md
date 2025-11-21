# Database Operations Guide

## Overview

Database operations are implemented as trait methods on the `Db` struct. This pattern provides clean separation and makes testing easier.

## Trait Pattern

### Base Trait

```rust
// db/mod.rs
pub trait DbOperations {
    fn conn(&self) -> &Arc<Mutex<Connection>>;
}

impl DbOperations for Db {
    fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }
}
```

### Feature Trait

```rust
// db/operations/conversations.rs
use rusqlite::{params, Result as RusqliteResult};
use super::DbOperations;

pub trait ConversationOperations: DbOperations {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>>;
    fn get_or_create_conversation(&self, id: &str) -> RusqliteResult<Conversation>;
    fn update_conversation_name(&self, id: &str, name: &str) -> RusqliteResult<()>;
    fn delete_conversation(&self, id: &str) -> RusqliteResult<()>;
}
```

### Implementation

```rust
impl ConversationOperations for Db {
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
}
```

## CRUD Operations

### Create

```rust
fn create_conversation(&self, id: &str, name: &str) -> RusqliteResult<Conversation> {
    let conn = self.conn().lock().unwrap();
    let created_at = Utc::now();
    let created_at_timestamp = created_at.timestamp();
    
    conn.execute(
        "INSERT INTO conversations (id, name, created_at) VALUES (?1, ?2, ?3)",
        params![id, name, created_at_timestamp],
    )?;
    
    Ok(Conversation {
        id: id.to_string(),
        name: name.to_string(),
        created_at,
    })
}
```

### Read (Single)

```rust
fn get_conversation(&self, id: &str) -> RusqliteResult<Conversation> {
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
    )
}
```

### Read (Multiple)

```rust
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
```

### Update

```rust
fn update_conversation_name(&self, id: &str, name: &str) -> RusqliteResult<()> {
    let conn = self.conn().lock().unwrap();
    
    conn.execute(
        "UPDATE conversations SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    
    Ok(())
}
```

### Delete

```rust
fn delete_conversation(&self, id: &str) -> RusqliteResult<()> {
    let conn = self.conn().lock().unwrap();
    
    conn.execute(
        "DELETE FROM conversations WHERE id = ?1",
        params![id],
    )?;
    
    Ok(())
}
```

## Transactions

### Multi-Step Operations

```rust
fn delete_conversation(&self, conversation_id: &str) -> RusqliteResult<()> {
    let mut conn = self.conn().lock().unwrap();
    let tx = conn.transaction()?;
    
    // Delete related data first (foreign keys)
    tx.execute(
        "DELETE FROM message_attachments WHERE message_id IN 
         (SELECT id FROM messages WHERE conversation_id = ?1)",
        params![conversation_id],
    )?;
    
    tx.execute(
        "DELETE FROM messages WHERE conversation_id = ?1",
        params![conversation_id],
    )?;
    
    // Delete main record
    tx.execute(
        "DELETE FROM conversations WHERE id = ?1",
        params![conversation_id],
    )?;
    
    tx.commit()?;
    Ok(())
}
```

### Atomic Batch Operations

```rust
fn batch_insert_messages(&self, messages: Vec<NewMessage>) -> RusqliteResult<()> {
    let mut conn = self.conn().lock().unwrap();
    let tx = conn.transaction()?;
    
    for message in messages {
        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message.id,
                message.conversation_id,
                message.role,
                message.content,
                message.created_at.timestamp()
            ],
        )?;
    }
    
    tx.commit()?;
    Ok(())
}
```

## Query Patterns

### With Conditions

```rust
fn get_messages_by_role(&self, conversation_id: &str, role: &str) -> RusqliteResult<Vec<Message>> {
    let conn = self.conn().lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, created_at 
         FROM messages 
         WHERE conversation_id = ?1 AND role = ?2
         ORDER BY created_at ASC"
    )?;
    
    let message_iter = stmt.query_map(params![conversation_id, role], |row| {
        Ok(Message {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            created_at: Utc.timestamp_opt(row.get(4)?, 0).single().unwrap(),
        })
    })?;
    
    message_iter.collect()
}
```

### With Joins

```rust
fn get_messages_with_attachments(&self, conversation_id: &str) -> RusqliteResult<Vec<MessageWithAttachments>> {
    let conn = self.conn().lock().unwrap();
    
    let mut stmt = conn.prepare(
        "SELECT m.id, m.content, m.role, m.created_at,
                a.id as attachment_id, a.filename, a.mime_type
         FROM messages m
         LEFT JOIN message_attachments a ON m.id = a.message_id
         WHERE m.conversation_id = ?1
         ORDER BY m.created_at ASC"
    )?;
    
    // Group attachments by message
    let mut messages: HashMap<String, MessageWithAttachments> = HashMap::new();
    
    let rows = stmt.query_map(params![conversation_id], |row| {
        Ok((
            row.get::<_, String>(0)?,  // message_id
            row.get::<_, String>(1)?,  // content
            row.get::<_, String>(2)?,  // role
            row.get::<_, i64>(3)?,     // created_at
            row.get::<_, Option<String>>(4)?,  // attachment_id
            row.get::<_, Option<String>>(5)?,  // filename
            row.get::<_, Option<String>>(6)?,  // mime_type
        ))
    })?;
    
    for row in rows {
        let (msg_id, content, role, timestamp, att_id, filename, mime_type) = row?;
        
        let message = messages.entry(msg_id.clone()).or_insert_with(|| {
            MessageWithAttachments {
                id: msg_id,
                content,
                role,
                created_at: Utc.timestamp_opt(timestamp, 0).single().unwrap(),
                attachments: Vec::new(),
            }
        });
        
        if let (Some(id), Some(name), Some(mime)) = (att_id, filename, mime_type) {
            message.attachments.push(Attachment { id, filename: name, mime_type: mime });
        }
    }
    
    Ok(messages.into_values().collect())
}
```

### Aggregations

```rust
fn get_conversation_stats(&self, conversation_id: &str) -> RusqliteResult<ConversationStats> {
    let conn = self.conn().lock().unwrap();
    
    let (message_count, total_tokens): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(token_count), 0)
         FROM messages
         WHERE conversation_id = ?1",
        params![conversation_id],
        |row| Ok((row.get(0)?, row.get(1)?))
    )?;
    
    Ok(ConversationStats {
        message_count: message_count as usize,
        total_tokens: total_tokens as usize,
    })
}
```

## Error Handling

### Using Custom Errors

```rust
fn get_conversation(&self, id: &str) -> Result<Conversation, DatabaseError> {
    let conn = self.conn().lock().unwrap();
    
    conn.query_row(
        "SELECT id, name, created_at FROM conversations WHERE id = ?1",
        params![id],
        |row| {
            // Mapping logic
        }
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            DatabaseError::ConversationNotFound(id.to_string())
        }
        _ => DatabaseError::Rusqlite(e)
    })
}
```

## Module Organization

```rust
// db/operations/mod.rs
mod conversations;
mod messages;
mod branches;

pub use conversations::ConversationOperations;
pub use messages::MessageOperations;
pub use branches::BranchOperations;
```

## Best Practices

### ✅ DO

- Use traits for all database operations
- Lock connection inside each method
- Use transactions for multi-step operations
- Handle timestamps correctly (i64 ↔ DateTime)
- Use `query_map` for collections
- Use `query_row` for single results
- Collect iterators at the end

### ❌ DON'T

- Hold locks across await points
- Use `.unwrap()` on database results
- Forget to commit transactions
- Mix business logic with database code
- Use string concatenation for SQL (use params!)
- Return rusqlite errors directly to commands

## Performance Tips

1. **Prepare statements once** in hot paths
2. **Use indexes** for frequent queries
3. **Batch operations** in transactions
4. **Avoid N+1 queries** - use joins
5. **Use EXPLAIN QUERY PLAN** to optimize
6. **Consider caching** for read-heavy data
