# Backend (Rust) Architecture Review

## Overview

The Rust backend follows a good trait-based operations pattern with clear separation between commands, operations, and models. However, there are several antipatterns and performance issues that need attention.

## Architecture Strengths

### Trait-Based Operations ✓
```rust
pub trait ConversationOperations: DbOperations {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>>
    // ...
}
```

Benefits:
- Good separation of concerns
- Testability (can mock traits)
- Clear boundaries between database and business logic

### Well-Organized Structure ✓
```
src-tauri/src/
├── main.rs           # Entry point
├── commands/         # Tauri command handlers
├── db/
│   ├── models/       # Data structures
│   └── operations/   # Database operations
└── files/            # File handling utilities
```

---

## Issues by Severity

### Critical

#### 1. Excessive Mutex Lock-Drop Pattern
**Location**: All operation files in `db/operations/*.rs`

Every database operation follows this antipattern:
```rust
let binding = self.conn();
let conn = binding.lock().unwrap();
// ... use conn ...
```

**Problems**:
1. The intermediate `binding` variable adds noise
2. `.unwrap()` on mutex locks can panic if poisoned
3. Locks held for entire function scope

**Fix**:
```rust
let conn = self.conn().lock()
    .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Mutex poisoned: {}", e)))?;
```

---

#### 2. Inconsistent Error Types
**Location**: `db/operations/branches.rs` and others

Mixed use of `RusqliteResult<T>` and `Result<T, DatabaseError>`:

```rust
// Some functions return RusqliteResult
fn create_branch(&self, ...) -> RusqliteResult<Branch>

// Others return DatabaseError
fn create_branch_from_message(...) -> Result<Branch, DatabaseError>
```

**Fix**: Standardize on `Result<T, DatabaseError>` for all operations.

---

#### 3. String-Based Error Conversion
**Location**: All command files (e.g., `commands/conversations.rs:9, 22, 28`)

All commands convert errors to String:
```rust
.map_err(|e| e.to_string())  // Loses structured error information
```

**Impact**: Frontend can't distinguish between different error types.

**Fix**: Define proper serializable error enum:
```rust
#[derive(Serialize)]
#[serde(tag = "type", content = "message")]
pub enum CommandError {
    DatabaseError(String),
    NotFound(String),
    ValidationError(String),
}
```

---

### Important

#### 4. Manual Lock Drop Antipattern
**Location**: `db/operations/branches.rs:303-315, 388-412`

Manually dropping locks indicates poor function decomposition:
```rust
drop(stmt);
drop(conn);
drop(binding);
self.create_branch(conversation_id, "Main")
```

**Fix**: Extract to separate functions with proper scoping.

---

#### 5. Incorrect created_at in update_system_prompt
**Location**: `db/operations/system_prompts.rs:30-47`

Returns current time as `created_at` instead of original value:
```rust
Ok(SystemPrompt {
    created_at: now,  // BUG! Should be original created_at
    updated_at: now,
})
```

**Fix**: Fetch complete record after update.

---

#### 6. Dynamic SQL Construction
**Location**: `db/operations/custom_backends.rs:83-114`

```rust
let sql = format!(
    "UPDATE custom_backends SET {} WHERE id = ?",
    updates.join(", ")
);
```

**Risk**: While field names are controlled here, this pattern is risky if extended.

---

#### 7. Inconsistent Timestamp Representation
**Location**: Various operation files

Custom backends uses `SystemTime` with milliseconds:
```rust
let created_at = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;  // Milliseconds
```

Other code uses `chrono` with seconds:
```rust
let created_at_timestamp = Utc::now().timestamp();  // Seconds
```

**Fix**: Standardize on `chrono` with seconds.

---

#### 8. Hardcoded App Identifier
**Location**:
- `db/operations/messages.rs:89`
- `files/mod.rs:52`

```rust
let attachments_dir = app_dir.join("dev.michalmlak.ai_agent").join("attachments");
```

**Fix**: Extract to constant or read from Tauri config.

---

#### 9. println! Debugging in Production
**Location**: Multiple files

```rust
println!("Tauri command update_conversation_name called with id={}, name={}", ...);
println!("📁 Setup time: {:?}", start_time.elapsed());
```

**Fix**: Use proper logging with log levels:
```rust
use log::{debug, info};
debug!("Update conversation name: id={}, name={}", ...);
```

---

### Minor

#### 10. Arc<Mutex<Connection>> May Be Overkill
**Location**: `db/mod.rs:14-22`

```rust
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}
```

The `Arc` might be unnecessary since Tauri's state management already wraps in `Arc`.

---

## Database Migrations

**Location**: `db/mod.rs`

276 lines of migrations with 39 steps. Contains tables that appear unused:
- `message_tool_executions`
- `message_agent_thinking`
- `users` (lines 41-45)
- `memories` (lines 46-50)

**Recommendation**: Audit which tables are actually used.

---

## Recommendations Summary

### High Priority
1. Fix N+1 queries (messages, usage)
2. Move file I/O outside database locks
3. Remove duplicate command registrations
4. Standardize error types

### Medium Priority
1. Improve mutex locking pattern
2. Fix panic-prone setup code
3. Standardize timestamp handling
4. Replace println! with proper logging

### Low Priority
1. Extract app identifier to constant
2. Refactor functions with manual drops
3. Review Arc<Mutex> necessity
4. Audit database migrations
