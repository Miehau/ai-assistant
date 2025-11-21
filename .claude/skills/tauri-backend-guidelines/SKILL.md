---
name: tauri-backend-guidelines
description: Comprehensive Tauri backend development guide for Rust/Tauri applications. Use when creating commands, database operations, error handling, state management, or working with Tauri APIs, rusqlite database access, file management, validation patterns, and async operations. Covers layered architecture (commands → operations → database), trait-based patterns, error handling, state management, testing strategies, and Tauri best practices.
---

# Tauri Backend Development Guidelines

## Purpose

Establish consistency and best practices for Tauri backend development using Rust, rusqlite, and modern Tauri patterns.

## When to Use This Skill

Automatically activates when working on:
- Creating or modifying Tauri commands and handlers
- Building database operations and queries
- Implementing traits for data access
- Database operations with rusqlite
- Error handling and custom error types
- State management with Tauri's State
- File management and processing
- Backend testing and refactoring

---

## Quick Start

### New Backend Feature Checklist

- [ ] **Command**: Clean function with `#[tauri::command]` macro
- [ ] **Operation Trait**: Database operation as trait method
- [ ] **Error Handling**: Custom error types with proper conversion
- [ ] **State**: Use `State<'_, T>` for managed state
- [ ] **Validation**: Input validation in commands
- [ ] **Tests**: Unit tests for operations
- [ ] **Registration**: Add to `invoke_handler!` in main.rs

### New Module Checklist

- [ ] Directory structure (commands/, db/, models/)
- [ ] Trait definitions in operations/
- [ ] Model structs with serde derives
- [ ] Error variants in db/error.rs
- [ ] Command functions in commands/
- [ ] Export in mod.rs files
- [ ] Register commands in main.rs

---

## Architecture Overview

### Layered Architecture

```
Tauri IPC Request
    ↓
Commands (public API)
    ↓
Operations (trait methods)
    ↓
Database (rusqlite)
```

**Key Principle:** Each layer has ONE responsibility.

**Current Structure:**
```
src-tauri/
├── src/
│   ├── commands/          # Tauri command handlers
│   │   ├── mod.rs
│   │   ├── conversations.rs
│   │   ├── messages.rs
│   │   ├── files.rs
│   │   └── ...
│   ├── db/                # Database layer
│   │   ├── mod.rs
│   │   ├── error.rs       # Custom error types
│   │   ├── models/        # Data models
│   │   └── operations/    # Database operations (traits)
│   ├── files/             # File management
│   ├── main.rs            # App entry point
│   └── setup_default_values.rs
├── Cargo.toml
└── tauri.conf.json
```

See [architecture-overview.md](resources/architecture-overview.md) for complete details.

---

## Core Principles (7 Key Rules)

### 1. Commands Only Handle IPC, Operations Do Work

```rust
// ❌ NEVER: Business logic in commands
#[tauri::command]
pub fn save_message(state: State<'_, Db>, data: String) -> Result<String, String> {
    let conn = state.conn().lock().unwrap();
    conn.execute("INSERT INTO messages...", params![data])?;
    // 50+ lines of logic
}

// ✅ ALWAYS: Delegate to operations
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

### 2. All Operations as Traits

```rust
// Define operations as trait methods
pub trait ConversationOperations: DbOperations {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>>;
    fn create_conversation(&self, id: &str) -> RusqliteResult<Conversation>;
    fn delete_conversation(&self, id: &str) -> RusqliteResult<()>;
}

// Implement on Db struct
impl ConversationOperations for Db {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>> {
        // Implementation
    }
}
```

### 3. Custom Error Types with Proper Conversions

```rust
#[derive(Debug)]
pub enum DatabaseError {
    Rusqlite(rusqlite::Error),
    MessageNotFound(String),
    ValidationError(String),
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(err: rusqlite::Error) -> Self {
        DatabaseError::Rusqlite(err)
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::Rusqlite(e) => write!(f, "Database error: {}", e),
            DatabaseError::MessageNotFound(id) => write!(f, "Message not found: {}", id),
            DatabaseError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}
```

### 4. Use Tauri State Management

```rust
// In main.rs setup
.setup(|app| {
    let db = Db::new(db_path)?;
    let file_manager = FileManager::new()?;
    
    app.manage(db);
    app.manage(file_manager);
    Ok(())
})

// In commands
#[tauri::command]
pub fn my_command(db: State<'_, Db>) -> Result<Data, String> {
    // Access state
    MyOperations::do_something(&*db)
        .map_err(|e| e.to_string())
}
```

### 5. Models with Serde Derives

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}
```

### 6. Transaction Support for Multi-Step Operations

```rust
fn delete_conversation(&self, conversation_id: &str) -> RusqliteResult<()> {
    let mut conn = self.conn().lock().unwrap();
    let tx = conn.transaction()?;
    
    // Multiple operations in transaction
    tx.execute("DELETE FROM message_attachments WHERE message_id IN (...)", params![conversation_id])?;
    tx.execute("DELETE FROM messages WHERE conversation_id = ?1", params![conversation_id])?;
    tx.execute("DELETE FROM conversations WHERE id = ?1", params![conversation_id])?;
    
    tx.commit()?;
    Ok(())
}
```

### 7. Register All Commands in main.rs

```rust
.invoke_handler(tauri::generate_handler![
    commands::get_conversations,
    commands::save_message,
    commands::delete_conversation,
    // ... all other commands
])
```

---

## Common Imports

```rust
// Tauri
use tauri::State;

// Database
use rusqlite::{params, Result as RusqliteResult, Connection};
use rusqlite_migration::{Migrations, M};

// Serialization
use serde::{Deserialize, Serialize};
use serde_json;

// Date/Time
use chrono::{DateTime, Utc, TimeZone};

// UUID
use uuid::Uuid;

// File handling
use std::fs;
use std::path::{Path, PathBuf};

// Error handling
use std::fmt;
use std::error::Error;
```

---

## Quick Reference

### Command Return Types

| Pattern | Use Case |
|---------|----------|
| `Result<T, String>` | Commands returning to frontend |
| `RusqliteResult<T>` | Database operations |
| `Result<T, DatabaseError>` | Custom error handling |
| `Result<(), String>` | Commands with no return value |

### State Management Patterns

```rust
// Single state
#[tauri::command]
pub fn cmd(db: State<'_, Db>) -> Result<Data, String> { }

// Multiple state
#[tauri::command]
pub fn cmd(
    db: State<'_, Db>,
    file_manager: State<'_, FileManager>
) -> Result<Data, String> { }
```

---

## Anti-Patterns to Avoid

❌ Business logic in commands
❌ Direct SQL in commands
❌ Using `.unwrap()` in commands (use proper error handling)
❌ Missing error conversions
❌ Forgetting to register commands
❌ No transaction for multi-step operations
❌ Mutable state without proper locking

---

## Navigation Guide

| Need to... | Read this |
|------------|-----------|
| Understand architecture | [architecture-overview.md](resources/architecture-overview.md) |
| Create commands | [commands-guide.md](resources/commands-guide.md) |
| Database operations | [database-operations.md](resources/database-operations.md) |
| Error handling | [error-handling.md](resources/error-handling.md) |
| State management | [state-management.md](resources/state-management.md) |
| File operations | [file-management.md](resources/file-management.md) |
| Testing | [testing-guide.md](resources/testing-guide.md) |
| Migrations | [database-migrations.md](resources/database-migrations.md) |
| See examples | [complete-examples.md](resources/complete-examples.md) |

---

## Resource Files

### [architecture-overview.md](resources/architecture-overview.md)
Layered architecture, request lifecycle, trait patterns

### [commands-guide.md](resources/commands-guide.md)
Command definitions, parameters, error handling, registration

### [database-operations.md](resources/database-operations.md)
Trait patterns, CRUD operations, transactions, queries

### [error-handling.md](resources/error-handling.md)
Custom error types, conversions, error propagation

### [state-management.md](resources/state-management.md)
Tauri State, initialization, thread safety, managed state

### [file-management.md](resources/file-management.md)
File operations, uploads, metadata, cleanup

### [testing-guide.md](resources/testing-guide.md)
Unit tests, integration tests, mocking

### [database-migrations.md](resources/database-migrations.md)
Migration patterns, schema changes, versioning

### [complete-examples.md](resources/complete-examples.md)
Full feature implementations, refactoring examples

---

**Skill Status**: COMPLETE ✅
**Line Count**: < 500 ✅
**Progressive Disclosure**: 9 resource files ✅
