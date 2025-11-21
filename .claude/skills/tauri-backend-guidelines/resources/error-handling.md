# Error Handling Guide

## Overview

Proper error handling in Tauri applications involves custom error types, conversions, and proper propagation between layers.

## Custom Error Types

### Database Error Enum

```rust
// db/error.rs
use rusqlite;
use std::fmt;

#[derive(Debug)]
pub enum DatabaseError {
    Rusqlite(rusqlite::Error),
    Migration(rusqlite_migration::Error),
    MessageNotFound(String),
    MessageNotInTree(String),
    ConversationNotFound(String),
    BranchNotFound(String),
    ValidationError(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::Rusqlite(e) => write!(f, "Database error: {}", e),
            DatabaseError::Migration(e) => write!(f, "Migration error: {}", e),
            DatabaseError::MessageNotFound(id) => write!(f, "Message not found: {}", id),
            DatabaseError::MessageNotInTree(id) => {
                write!(f, "Message exists but is not in the message tree: {}. This conversation may need repair.", id)
            }
            DatabaseError::ConversationNotFound(id) => write!(f, "Conversation not found: {}", id),
            DatabaseError::BranchNotFound(id) => write!(f, "Branch not found: {}", id),
            DatabaseError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for DatabaseError {}
```

### Error Conversions

```rust
impl From<rusqlite::Error> for DatabaseError {
    fn from(err: rusqlite::Error) -> Self {
        DatabaseError::Rusqlite(err)
    }
}

impl From<rusqlite_migration::Error> for DatabaseError {
    fn from(err: rusqlite_migration::Error) -> Self {
        DatabaseError::Migration(err)
    }
}
```

## Error Propagation

### In Operations Layer

```rust
// Return custom error type
pub trait MessageOperations: DbOperations {
    fn get_message(&self, id: &str) -> Result<Message, DatabaseError> {
        let conn = self.conn().lock().unwrap();
        
        conn.query_row(
            "SELECT id, content FROM messages WHERE id = ?1",
            params![id],
            |row| {
                Ok(Message {
                    id: row.get(0)?,
                    content: row.get(1)?,
                })
            }
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                DatabaseError::MessageNotFound(id.to_string())
            }
            _ => DatabaseError::Rusqlite(e)
        })
    }
}
```

### In Commands Layer

```rust
// Convert to String for frontend
#[tauri::command]
pub fn get_message(state: State<'_, Db>, id: String) -> Result<Message, String> {
    MessageOperations::get_message(&*state, &id)
        .map_err(|e| e.to_string())  // Convert DatabaseError to String
}
```

## Error Handling Patterns

### Basic Try Operator

```rust
fn create_with_validation(&self, data: &str) -> Result<Item, DatabaseError> {
    // Validate first
    if data.is_empty() {
        return Err(DatabaseError::ValidationError("Data cannot be empty".to_string()));
    }
    
    let conn = self.conn().lock().unwrap();
    
    // Use ? operator for automatic conversion
    conn.execute(
        "INSERT INTO items (data) VALUES (?1)",
        params![data],
    )?;  // Automatically converts rusqlite::Error to DatabaseError
    
    Ok(item)
}
```

### Match for Specific Errors

```rust
fn get_or_create(&self, id: &str) -> Result<Item, DatabaseError> {
    match self.get_item(id) {
        Ok(item) => Ok(item),
        Err(DatabaseError::MessageNotFound(_)) => {
            // Not found, create new
            self.create_item(id)
        }
        Err(e) => Err(e),  // Propagate other errors
    }
}
```

### map_err for Custom Messages

```rust
fn complex_operation(&self, id: &str) -> Result<(), DatabaseError> {
    self.step_one(id)
        .map_err(|e| DatabaseError::ValidationError(
            format!("Step one failed for {}: {}", id, e)
        ))?;
    
    self.step_two(id)
        .map_err(|e| DatabaseError::ValidationError(
            format!("Step two failed for {}: {}", id, e)
        ))?;
    
    Ok(())
}
```

## Validation Errors

### Input Validation

```rust
fn validate_conversation_name(name: &str) -> Result<(), DatabaseError> {
    if name.is_empty() {
        return Err(DatabaseError::ValidationError(
            "Conversation name cannot be empty".to_string()
        ));
    }
    
    if name.len() > 255 {
        return Err(DatabaseError::ValidationError(
            format!("Conversation name too long: {} characters (max 255)", name.len())
        ));
    }
    
    Ok(())
}

fn update_conversation_name(&self, id: &str, name: &str) -> Result<(), DatabaseError> {
    validate_conversation_name(name)?;  // Validate first
    
    let conn = self.conn().lock().unwrap();
    conn.execute(
        "UPDATE conversations SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    
    Ok(())
}
```

### Business Logic Validation

```rust
fn delete_branch(&self, branch_id: &str) -> Result<(), DatabaseError> {
    let conn = self.conn().lock().unwrap();
    
    // Check if branch is main
    let is_main: bool = conn.query_row(
        "SELECT is_main FROM branches WHERE id = ?1",
        params![branch_id],
        |row| row.get(0)
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            DatabaseError::BranchNotFound(branch_id.to_string())
        }
        _ => DatabaseError::Rusqlite(e)
    })?;
    
    if is_main {
        return Err(DatabaseError::ValidationError(
            "Cannot delete the main branch".to_string()
        ));
    }
    
    conn.execute("DELETE FROM branches WHERE id = ?1", params![branch_id])?;
    Ok(())
}
```

## Transaction Error Handling

### Rollback on Error

```rust
fn atomic_operation(&self, id: &str) -> Result<(), DatabaseError> {
    let mut conn = self.conn().lock().unwrap();
    let tx = conn.transaction()?;
    
    // Multiple operations
    tx.execute("DELETE FROM related WHERE parent_id = ?1", params![id])?;
    tx.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    
    // Commit - if this fails, transaction rolls back automatically
    tx.commit()?;
    
    Ok(())
}
```

### Manual Rollback

```rust
fn complex_transaction(&self, items: Vec<Item>) -> Result<(), DatabaseError> {
    let mut conn = self.conn().lock().unwrap();
    let tx = conn.transaction()?;
    
    for item in items {
        if let Err(e) = tx.execute(
            "INSERT INTO items (id, data) VALUES (?1, ?2)",
            params![item.id, item.data],
        ) {
            // Explicitly rollback
            tx.rollback()?;
            return Err(DatabaseError::ValidationError(
                format!("Failed to insert item {}: {}", item.id, e)
            ));
        }
    }
    
    tx.commit()?;
    Ok(())
}
```

## Logging and Debugging

### Debug Logging in Commands

```rust
#[tauri::command]
pub fn delete_conversation(state: State<'_, Db>, conversation_id: String) -> Result<(), String> {
    println!("Tauri command delete_conversation called with id={}", conversation_id);
    
    ConversationOperations::delete_conversation(&*state, &conversation_id)
        .map_err(|e| {
            eprintln!("Error in delete_conversation command: {}", e);
            e.to_string()
        })
}
```

### Detailed Error Context

```rust
impl DatabaseError {
    pub fn with_context(self, context: &str) -> Self {
        match self {
            DatabaseError::Rusqlite(e) => {
                DatabaseError::ValidationError(format!("{}: {}", context, e))
            }
            _ => self,
        }
    }
}

// Usage
fn my_operation(&self) -> Result<(), DatabaseError> {
    self.sub_operation()
        .map_err(|e| e.with_context("Failed during sub_operation"))?;
    Ok(())
}
```

## File Operation Errors

### File Error Type

```rust
#[derive(Debug)]
pub enum FileError {
    Io(std::io::Error),
    InvalidFormat(String),
    FileTooLarge { size: usize, max: usize },
    UnsupportedType(String),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::Io(e) => write!(f, "IO error: {}", e),
            FileError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            FileError::FileTooLarge { size, max } => {
                write!(f, "File too large: {} bytes (max: {} bytes)", size, max)
            }
            FileError::UnsupportedType(mime) => write!(f, "Unsupported file type: {}", mime),
        }
    }
}

impl From<std::io::Error> for FileError {
    fn from(err: std::io::Error) -> Self {
        FileError::Io(err)
    }
}
```

## Best Practices

### ✅ DO

- Create specific error variants for different scenarios
- Implement `Display` trait for user-friendly messages
- Implement `Error` trait for standard error handling
- Use `From` trait for automatic conversions
- Convert errors to `String` only in command layer
- Include context in error messages
- Log errors before returning them
- Use `?` operator for cleaner code

### ❌ DON'T

- Use `.unwrap()` in production code
- Return generic error messages
- Lose error context during conversion
- Panic in commands or operations
- Ignore errors with `let _ =`
- Use string errors in operations layer
- Forget to implement `std::error::Error`

## Error Recovery Patterns

### Retry Logic

```rust
fn with_retry<F, T>(mut operation: F, max_attempts: u32) -> Result<T, DatabaseError>
where
    F: FnMut() -> Result<T, DatabaseError>,
{
    let mut attempts = 0;
    loop {
        match operation() {
            Ok(result) => return Ok(result),
            Err(DatabaseError::Rusqlite(rusqlite::Error::SqliteFailure(err, _))) 
                if err.code == rusqlite::ErrorCode::DatabaseBusy => {
                attempts += 1;
                if attempts >= max_attempts {
                    return Err(DatabaseError::ValidationError(
                        format!("Database busy after {} attempts", attempts)
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(100 * attempts as u64));
            }
            Err(e) => return Err(e),
        }
    }
}
```

### Fallback Values

```rust
fn get_or_default(&self, id: &str) -> Result<Item, DatabaseError> {
    match self.get_item(id) {
        Ok(item) => Ok(item),
        Err(DatabaseError::MessageNotFound(_)) => Ok(Item::default()),
        Err(e) => Err(e),
    }
}
```

## Testing Error Handling

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_error() {
        let db = Db::new_test();
        let result = MessageOperations::get_message(&db, "nonexistent");
        
        assert!(matches!(result, Err(DatabaseError::MessageNotFound(_))));
    }

    #[test]
    fn test_validation_error() {
        let db = Db::new_test();
        let result = ConversationOperations::update_conversation_name(&db, "id", "");
        
        assert!(matches!(result, Err(DatabaseError::ValidationError(_))));
    }
}
```
