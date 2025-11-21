# State Management Guide

## Overview

Tauri's state management allows you to share application state across commands using managed state with thread-safe access.

## Managed State Pattern

### Initialization in main.rs

```rust
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Initialize database
            let app_dir = app.path_resolver().app_data_dir()
                .expect("Failed to get app data dir");
            fs::create_dir_all(&app_dir)
                .expect("Failed to create app directory");
            
            let db_path = app_dir.join("app.db");
            let mut db = Db::new(db_path.to_str().unwrap())
                .expect("Failed to create database");
            db.run_migrations()
                .expect("Failed to run database migrations");
            
            // Initialize file manager
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

## Database State

### Db Struct with Thread Safety

```rust
// db/mod.rs
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

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

### Accessing State in Commands

```rust
#[tauri::command]
pub fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    // Access the state
    ConversationOperations::get_conversations(&*state)
        .map_err(|e| e.to_string())
}
```

## Multiple State Access

### Commands with Multiple State

```rust
#[tauri::command]
pub fn upload_file(
    db: State<'_, Db>,
    file_manager: State<'_, FileManager>,
    conversation_id: String,
    file_data: Vec<u8>,
    filename: String,
) -> Result<FileInfo, String> {
    // Use both states
    let file_info = file_manager
        .save_file(&conversation_id, &file_data, &filename)
        .map_err(|e| e.to_string())?;
    
    // Record in database
    db.record_file_upload(&file_info.id, &conversation_id)
        .map_err(|e| e.to_string())?;
    
    Ok(file_info)
}
```

## Thread Safety

### Mutex Pattern

```rust
// Always lock, use, and immediately release
pub trait MessageOperations: DbOperations {
    fn get_message(&self, id: &str) -> RusqliteResult<Message> {
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
        )
        // Lock is automatically released here
    }
}
```

### Don't Hold Locks Too Long

```rust
// ❌ BAD: Holding lock while doing other work
fn bad_pattern(&self, id: &str) -> Result<(), DatabaseError> {
    let conn = self.conn().lock().unwrap();
    
    // Process data (expensive operation)
    let processed = expensive_computation();
    
    // Still holding lock!
    conn.execute("INSERT INTO ...", params![processed])?;
    Ok(())
}

// ✅ GOOD: Minimize lock duration
fn good_pattern(&self, id: &str) -> Result<(), DatabaseError> {
    // Process data first
    let processed = expensive_computation();
    
    // Lock only when needed
    let conn = self.conn().lock().unwrap();
    conn.execute("INSERT INTO ...", params![processed])?;
    Ok(())
}
```

## Custom State Types

### File Manager State

```rust
// files/mod.rs
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct FileManager {
    base_path: PathBuf,
    cache: Arc<Mutex<HashMap<String, CachedFile>>>,
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
        
        Ok(FileManager {
            base_path,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    pub fn save_file(&self, conversation_id: &str, data: &[u8], filename: &str) 
        -> Result<FileInfo, FileError> {
        let path = self.base_path
            .join(conversation_id)
            .join(filename);
        
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, data)?;
        
        // Update cache
        let mut cache = self.cache.lock().unwrap();
        cache.insert(filename.to_string(), CachedFile { 
            path: path.clone(),
            size: data.len(),
        });
        
        Ok(FileInfo {
            id: uuid::Uuid::new_v4().to_string(),
            filename: filename.to_string(),
            path: path.to_string_lossy().to_string(),
        })
    }
}
```

### Configuration State

```rust
pub struct AppConfig {
    settings: Arc<Mutex<Settings>>,
}

impl AppConfig {
    pub fn new() -> Self {
        AppConfig {
            settings: Arc::new(Mutex::new(Settings::default())),
        }
    }
    
    pub fn get_setting(&self, key: &str) -> Option<String> {
        let settings = self.settings.lock().unwrap();
        settings.get(key).cloned()
    }
    
    pub fn set_setting(&self, key: String, value: String) {
        let mut settings = self.settings.lock().unwrap();
        settings.insert(key, value);
    }
}
```

## State Initialization Patterns

### With Setup Values

```rust
.setup(|app| {
    let db = Db::new(db_path)?;
    
    // Initialize default values
    setup_default_values::initialize(&mut db)
        .expect("Failed to initialize default values");
    
    app.manage(db);
    Ok(())
})
```

### Lazy Initialization

```rust
use std::sync::Once;

static INIT: Once = Once::new();

pub struct LazyState {
    data: Arc<Mutex<Option<ExpensiveResource>>>,
}

impl LazyState {
    pub fn new() -> Self {
        LazyState {
            data: Arc::new(Mutex::new(None)),
        }
    }
    
    pub fn get(&self) -> Arc<ExpensiveResource> {
        let mut data = self.data.lock().unwrap();
        
        if data.is_none() {
            INIT.call_once(|| {
                *data = Some(ExpensiveResource::initialize());
            });
        }
        
        Arc::clone(data.as_ref().unwrap())
    }
}
```

## Accessing App Paths

### Using Path Resolver

```rust
.setup(|app| {
    // App data directory
    let app_dir = app.path_resolver()
        .app_data_dir()
        .expect("Failed to get app data dir");
    
    // App config directory
    let config_dir = app.path_resolver()
        .app_config_dir()
        .expect("Failed to get config dir");
    
    // App cache directory
    let cache_dir = app.path_resolver()
        .app_cache_dir()
        .expect("Failed to get cache dir");
    
    // Resource directory (for bundled files)
    let resource_dir = app.path_resolver()
        .resource_dir()
        .expect("Failed to get resource dir");
    
    Ok(())
})
```

## State Cleanup

### Drop Implementation

```rust
impl Drop for FileManager {
    fn drop(&mut self) {
        // Cleanup temporary files
        if let Err(e) = self.cleanup_temp_files() {
            eprintln!("Error cleaning up temp files: {}", e);
        }
    }
}
```

### Explicit Cleanup Command

```rust
#[tauri::command]
pub fn cleanup(
    db: State<'_, Db>,
    file_manager: State<'_, FileManager>
) -> Result<(), String> {
    // Cleanup files
    file_manager.cleanup_old_files()
        .map_err(|e| e.to_string())?;
    
    // Vacuum database
    db.vacuum()
        .map_err(|e| e.to_string())?;
    
    Ok(())
}
```

## Best Practices

### ✅ DO

- Use `Arc<Mutex<T>>` for shared mutable state
- Initialize all state in `.setup()`
- Use `State<'_, T>` for command parameters
- Lock only when necessary
- Release locks quickly
- Use meaningful state type names
- Implement cleanup logic
- Handle initialization errors

### ❌ DON'T

- Hold locks across await points
- Share raw pointers
- Use global mutable state
- Forget to register state with `.manage()`
- Panic in initialization
- Access uninitialized state
- Use `RefCell` in multi-threaded context
- Keep locks during I/O operations

## Testing with State

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
    fn test_with_state() {
        let db = create_test_db();
        
        // Test operations
        let result = ConversationOperations::create_conversation(&db, "test-id");
        assert!(result.is_ok());
    }
}
```

## Advanced Patterns

### State with Events

```rust
use tauri::{Manager, Window};

pub struct AppState {
    db: Arc<Mutex<Db>>,
    window: Arc<Mutex<Option<Window>>>,
}

impl AppState {
    pub fn emit_event(&self, event: &str, payload: impl Serialize) {
        if let Some(window) = self.window.lock().unwrap().as_ref() {
            let _ = window.emit(event, payload);
        }
    }
}

// In setup
.setup(|app| {
    let window = app.get_window("main").unwrap();
    let state = AppState {
        db: Arc::new(Mutex::new(db)),
        window: Arc::new(Mutex::new(Some(window))),
    };
    app.manage(state);
    Ok(())
})
```
