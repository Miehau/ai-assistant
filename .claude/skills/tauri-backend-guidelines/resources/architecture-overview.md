# Architecture Overview

## Layered Architecture

The Tauri backend follows a clean layered architecture pattern:

```
┌─────────────────────────────────┐
│     Frontend (TypeScript)       │
│         Svelte/React            │
└────────────┬────────────────────┘
             │ IPC (Tauri invoke)
             ▼
┌─────────────────────────────────┐
│    Commands Layer (Rust)        │
│  - Thin wrappers                │
│  - Parameter validation         │
│  - Error conversion             │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│   Operations Layer (Traits)     │
│  - Business logic               │
│  - Data access                  │
│  - Transaction management       │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│    Database (rusqlite)          │
│  - SQLite connection            │
│  - Thread-safe access           │
└─────────────────────────────────┘
```

## Request Lifecycle

### 1. Frontend Invokes Command

```typescript
import { invoke } from '@tauri-apps/api/tauri';

const conversations = await invoke<Conversation[]>('get_conversations');
```

### 2. Command Receives Request

```rust
#[tauri::command]
pub fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    // Delegate to operations
    ConversationOperations::get_conversations(&*state)
        .map_err(|e| e.to_string())
}
```

### 3. Operation Performs Work

```rust
impl ConversationOperations for Db {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>> {
        let conn = self.conn().lock().unwrap();
        // Database logic here
    }
}
```

### 4. Response Returns Through Layers

```
Database Result
    ↓
Operation (RusqliteResult<Vec<Conversation>>)
    ↓
Command (Result<Vec<Conversation>, String>)
    ↓
Frontend (Promise<Conversation[]>)
```

## Directory Structure

```
src-tauri/
├── src/
│   ├── main.rs                 # App entry point, state initialization
│   │
│   ├── commands/               # Command handlers (IPC layer)
│   │   ├── mod.rs
│   │   ├── conversations.rs
│   │   ├── messages.rs
│   │   ├── files.rs
│   │   ├── branches.rs
│   │   └── usage.rs
│   │
│   ├── db/                     # Database layer
│   │   ├── mod.rs              # Db struct, DbOperations trait
│   │   ├── error.rs            # Custom error types
│   │   │
│   │   ├── models/             # Data models
│   │   │   ├── mod.rs
│   │   │   ├── conversation.rs
│   │   │   ├── message.rs
│   │   │   ├── branch.rs
│   │   │   └── usage.rs
│   │   │
│   │   └── operations/         # Database operations (traits)
│   │       ├── mod.rs
│   │       ├── conversations.rs
│   │       ├── messages.rs
│   │       ├── branches.rs
│   │       └── usage.rs
│   │
│   ├── files/                  # File management
│   │   ├── mod.rs
│   │   ├── audio.rs
│   │   ├── image.rs
│   │   ├── text.rs
│   │   └── versioning.rs
│   │
│   ├── setup_default_values.rs # Initial data setup
│   └── build.rs                # Build script
│
├── Cargo.toml                  # Dependencies
├── tauri.conf.json             # Tauri configuration
└── icons/                      # App icons
```

## Separation of Concerns

### Commands Layer

**Responsibilities:**
- Receive requests from frontend
- Validate input parameters
- Delegate to operations
- Convert errors to `String`
- Optional logging

**Should NOT:**
- Contain business logic
- Access database directly
- Perform complex computations

```rust
// ✅ GOOD: Thin command
#[tauri::command]
pub fn delete_conversation(
    state: State<'_, Db>,
    conversation_id: String
) -> Result<(), String> {
    ConversationOperations::delete_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

// ❌ BAD: Fat command with logic
#[tauri::command]
pub fn delete_conversation(
    state: State<'_, Db>,
    conversation_id: String
) -> Result<(), String> {
    let conn = state.conn().lock().unwrap();
    // 50 lines of SQL and logic...
}
```

### Operations Layer

**Responsibilities:**
- Business logic implementation
- Database queries
- Transaction management
- Data transformation
- Error handling with custom types

**Should NOT:**
- Know about Tauri commands
- Return `String` errors (use proper error types)
- Handle IPC concerns

```rust
// ✅ GOOD: Clean operation
pub trait ConversationOperations: DbOperations {
    fn delete_conversation(&self, id: &str) -> RusqliteResult<()> {
        let mut conn = self.conn().lock().unwrap();
        let tx = conn.transaction()?;
        
        tx.execute("DELETE FROM message_attachments WHERE ...", params![id])?;
        tx.execute("DELETE FROM messages WHERE conversation_id = ?1", params![id])?;
        tx.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        
        tx.commit()?;
        Ok(())
    }
}
```

### Database Layer

**Responsibilities:**
- Connection management
- Thread-safe access via Mutex
- Migration execution

```rust
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn new(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
    
    pub fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }
}
```

## Trait-Based Operations

### Base Trait

All operation traits extend `DbOperations`:

```rust
pub trait DbOperations {
    fn conn(&self) -> &Arc<Mutex<Connection>>;
}

impl DbOperations for Db {
    fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }
}
```

### Feature Traits

Each feature has its own operations trait:

```rust
pub trait ConversationOperations: DbOperations {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>>;
    fn create_conversation(&self, id: &str) -> RusqliteResult<Conversation>;
    fn delete_conversation(&self, id: &str) -> RusqliteResult<()>;
}

pub trait MessageOperations: DbOperations {
    fn save_message(&self, data: &MessageData) -> RusqliteResult<String>;
    fn get_messages(&self, conversation_id: &str) -> RusqliteResult<Vec<Message>>;
}

pub trait BranchOperations: DbOperations {
    fn create_branch(&self, data: &BranchData) -> RusqliteResult<Branch>;
    fn get_branches(&self, conversation_id: &str) -> RusqliteResult<Vec<Branch>>;
}
```

### Implementation

All traits are implemented on the `Db` struct:

```rust
impl ConversationOperations for Db {}
impl MessageOperations for Db {}
impl BranchOperations for Db {}
```

## State Management

### Initialization

```rust
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Create database
            let app_dir = app.path_resolver().app_data_dir()
                .expect("Failed to get app data dir");
            fs::create_dir_all(&app_dir)
                .expect("Failed to create app directory");
            
            let db_path = app_dir.join("app.db");
            let mut db = Db::new(db_path.to_str().unwrap())
                .expect("Failed to create database");
            
            // Run migrations
            db.run_migrations()
                .expect("Failed to run database migrations");
            
            // Initialize defaults
            setup_default_values::initialize(&mut db)
                .expect("Failed to initialize default values");
            
            // Create file manager
            let file_manager = FileManager::new()
                .expect("Failed to create file manager");
            
            // Register managed state
            app.manage(db);
            app.manage(file_manager);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // commands...
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Accessing State

```rust
// Single state
#[tauri::command]
pub fn cmd(db: State<'_, Db>) -> Result<Data, String> {
    // Use &*db to get &Db reference
}

// Multiple state
#[tauri::command]
pub fn cmd(
    db: State<'_, Db>,
    file_manager: State<'_, FileManager>
) -> Result<Data, String> {
    // Use both states
}
```

## Thread Safety

### Mutex Usage

```rust
// Lock, use, release pattern
fn get_data(&self) -> RusqliteResult<Data> {
    let conn = self.conn().lock().unwrap();
    
    conn.query_row("SELECT ...", [], |row| {
        // mapping
    })
    // Lock automatically released here
}
```

### Transaction Safety

```rust
fn atomic_operation(&self) -> RusqliteResult<()> {
    let mut conn = self.conn().lock().unwrap();
    let tx = conn.transaction()?;
    
    // Multiple operations
    tx.execute("...", params![])?;
    tx.execute("...", params![])?;
    
    tx.commit()?;  // Rolls back automatically on error
    Ok(())
}
```

## Error Flow

### Custom Error Types

```rust
#[derive(Debug)]
pub enum DatabaseError {
    Rusqlite(rusqlite::Error),
    NotFound(String),
    ValidationError(String),
}
```

### Error Conversion Flow

```
rusqlite::Error
    ↓ (From trait)
DatabaseError
    ↓ (map_err + to_string)
String (for frontend)
```

```rust
// In operation
fn get_item(&self, id: &str) -> Result<Item, DatabaseError> {
    conn.query_row(...)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                DatabaseError::NotFound(id.to_string())
            }
            _ => DatabaseError::Rusqlite(e)
        })
}

// In command
#[tauri::command]
pub fn get_item(state: State<'_, Db>, id: String) -> Result<Item, String> {
    Operations::get_item(&*state, &id)
        .map_err(|e| e.to_string())
}
```

## Best Practices

### ✅ DO

- Keep commands thin (< 10 lines)
- Put all business logic in operations
- Use traits for all database access
- Lock database connections briefly
- Use transactions for multi-step operations
- Convert errors properly at each layer
- Register all commands in main.rs

### ❌ DON'T

- Put business logic in commands
- Access database directly from commands
- Hold locks across operations
- Use `.unwrap()` in production code
- Return internal error types to frontend
- Mix concerns between layers
- Forget to register new commands

## Module Organization

```rust
// commands/mod.rs
mod conversations;
mod messages;

pub use conversations::*;
pub use messages::*;

// db/operations/mod.rs
mod conversations;
mod messages;

pub use conversations::ConversationOperations;
pub use messages::MessageOperations;

// db/models/mod.rs
mod conversation;
mod message;

pub use conversation::Conversation;
pub use message::Message;
```

This architecture ensures:
- Clear separation of concerns
- Easy testing (mock traits)
- Type safety
- Thread safety
- Maintainability
- Scalability
