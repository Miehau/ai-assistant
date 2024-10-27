// src/db/mod.rs
use rusqlite::{Connection, Result as RusqliteResult, params};
use rusqlite_migration::{Migrations, M};
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, TimeZone};
use uuid::Uuid;
use serde_json::Value as JsonValue;

// Define a custom error type
#[derive(Debug)]
pub enum DatabaseError {
    Rusqlite(rusqlite::Error),
    Migration(rusqlite_migration::Error),
}

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub role: String,
    pub conversation_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Model {
    pub id: String,
    pub provider: String,
    pub api_key: Option<String>,
    pub model_name: String,
    pub alias: String,
    pub url: Option<String>,
    pub deployment_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn new(db_path: &str) -> Result<Self, DatabaseError> {
        let conn = Connection::open(db_path)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)), })
    }

    pub fn run_migrations(&mut self) -> Result<(), DatabaseError> {
        // Define your migrations
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
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                api_key TEXT,
                model_name TEXT NOT NULL,
                alias TEXT NOT NULL,
                url TEXT,
                deployment_name TEXT,
                created_at INTEGER NOT NULL
            );"),
        ]);

        let mut conn = self.conn.lock().unwrap();
        migrations.to_latest(&mut *conn)?;
        Ok(())
    }

    pub fn save_memory(&self, content: &str) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memories (content) VALUES (?1)",
            params![content],
        )?;
        Ok(())
    }

    pub fn save_message(&self, conversation_id: &str, role: &str, content: &str) -> RusqliteResult<()> {
        let message_id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        let created_at_timestamp = created_at.timestamp();  // Convert to Unix timestamp
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![message_id, conversation_id, role, content, created_at_timestamp],
        )?;
        Ok(())
    }

    pub fn get_messages(&self, conversation_id: &str) -> RusqliteResult<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC"
        )?;
        let message_iter = stmt.query_map(params![conversation_id], |row| {
            let timestamp: i64 = row.get(4)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at,
            })
        })?;
        message_iter.collect()
    }

    pub fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>> {
        let conn = self.conn.lock().unwrap();
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

    pub fn get_or_create_conversation(&self, conversation_id: &str) -> RusqliteResult<Conversation> {
        let conn = self.conn.lock().unwrap();
        
        // First, try to get the existing conversation
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
                // Conversation doesn't exist, create a new one
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

    pub fn add_model(&self, model: &Model) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO models (id, provider, api_key, model_name, alias, url, deployment_name, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                model.id,
                model.provider,
                model.api_key,
                model.model_name,
                model.alias,
                model.url,
                model.deployment_name,
                model.created_at.timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn get_models(&self) -> RusqliteResult<Vec<Model>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider, api_key, model_name, alias, url, deployment_name, created_at FROM models"
        )?;
        let model_iter = stmt.query_map([], |row| {
            let timestamp: i64 = row.get(7)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            Ok(Model {
                id: row.get(0)?,
                provider: row.get(1)?,
                api_key: row.get(2)?,
                model_name: row.get(3)?,
                alias: row.get(4)?,
                url: row.get(5)?,
                deployment_name: row.get(6)?,
                created_at,
            })
        })?;
        model_iter.collect()
    }
}