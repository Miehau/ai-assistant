// src/db/mod.rs
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::sync::{Arc, Mutex};

mod error;
mod models;
mod operations;

pub use error::DatabaseError;
pub use models::*;
pub use operations::*;

pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl DbOperations for Db {
    fn conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }
}

impl MessageOperations for Db {}
impl ConversationOperations for Db {}
impl ModelOperations for Db {}
impl SystemPromptOperations for Db {}

impl Db {
    pub fn new(db_path: &str) -> Result<Self, DatabaseError> {
        let conn = Connection::open(db_path)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn run_migrations(&mut self) -> Result<(), DatabaseError> {
        let migrations = Migrations::new(vec![
            M::up("CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE
            );"),
            M::up("CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );"),
            M::up("CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );"),
            M::up("CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                role TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            );"),
            M::up("CREATE TABLE IF NOT EXISTS models (
                provider TEXT NOT NULL,
                model_name TEXT NOT NULL,
                url TEXT,
                deployment_name TEXT,
                enabled BOOLEAN NOT NULL DEFAULT 0,
                PRIMARY KEY (provider, model_name)
            );"),
            M::up("CREATE TABLE IF NOT EXISTS api_keys (
                provider TEXT PRIMARY KEY,
                key TEXT NOT NULL
            );"),
            M::up("CREATE TABLE IF NOT EXISTS system_prompts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT 'Untitled',
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );"),
            M::up("CREATE TABLE IF NOT EXISTS message_attachments (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                name TEXT NOT NULL,
                data TEXT NOT NULL,
                attachment_type TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id)
            );"),
            M::up("ALTER TABLE message_attachments ADD COLUMN thumbnail_path TEXT;"),
        ]);

        let mut conn = self.conn.lock().unwrap();
        migrations.to_latest(&mut *conn)?;
        Ok(())
    }
}
