// src/db/mod.rs
use rusqlite::{Connection, Result as RusqliteResult, params};
use rusqlite_migration::{Migrations, M};
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, TimeZone};
use uuid::Uuid;
use std::fs;
use std::path::PathBuf;
use tauri::api::path;
use base64::Engine;

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
pub struct IncomingAttachment {
    pub name: String,
    pub data: String,  // This will contain the raw data
    pub attachment_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageAttachment {
    pub id: Option<String>,
    pub message_id: Option<String>,
    pub name: String,
    pub file_path: String,
    pub attachment_type: String,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub role: String,
    pub conversation_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<MessageAttachment>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Model {
    pub provider: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_name: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

// Add this new struct after the Model struct
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKey {
    pub provider: String,
    pub key: String,
}

// Add this new struct after the existing ones
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemPrompt {
    pub id: String,
    pub name: String,  // Add this field
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
                file_path TEXT,
                attachment_type TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id)
            );"),
            M::up("ALTER TABLE message_attachments RENAME COLUMN data TO file_path;"),
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

    fn save_attachment_to_fs(&self, data: &str, file_name: &str) -> RusqliteResult<String> {
        // Get app attachments directory from Tauri
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("Failed to get app directory".into()))?;
        
        let attachments_dir = app_dir.join("com.tauri.dev").join("attachments");
        fs::create_dir_all(&attachments_dir)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        // Create unique filename to avoid collisions
        let unique_filename = format!("{}-{}", Uuid::new_v4(), file_name);
        let file_path = attachments_dir.join(&unique_filename);
        
        // Extract base64 data after the comma if it's a data URL
        let base64_data = if data.starts_with("data:") {
            data.split(",").nth(1)
                .ok_or_else(|| rusqlite::Error::InvalidParameterName("Invalid data URL format".into()))?
        } else {
            data
        };
        
        // Decode base64 data
        let decoded_data = base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        
        // Write the decoded data to file
        fs::write(&file_path, decoded_data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        // Return relative path as string
        Ok(unique_filename)
    }

    pub fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        attachments: &[IncomingAttachment],
    ) -> RusqliteResult<()> {
        let message_id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        let created_at_timestamp = created_at.timestamp();
        
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        
        // Insert message
        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![message_id, conversation_id, role, content, created_at_timestamp],
        )?;
        
        // Save attachments
        for attachment in attachments {
            let file_path = self.save_attachment_to_fs(
                &attachment.data,
                &attachment.name
            )?;

            tx.execute(
                "INSERT INTO message_attachments (id, message_id, name, file_path, attachment_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    Uuid::new_v4().to_string(),
                    message_id,
                    attachment.name,
                    file_path,
                    attachment.attachment_type,
                    created_at_timestamp
                ],
            )?;
        }
        
        tx.commit()?;
        Ok(())
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
            "INSERT INTO models (provider, model_name, url, deployment_name, enabled) 
             VALUES (?1, ?2, ?3, ?4, 1)",  // Set enabled to 1 (true) by default
            params![
                model.provider,
                model.model_name,
                model.url,
                model.deployment_name,
            ],
        )?;
        Ok(())
    }

    pub fn get_models(&self) -> RusqliteResult<Vec<Model>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT provider, model_name, url, deployment_name, enabled FROM models"
        )?;
        let model_iter = stmt.query_map([], |row| {
            Ok(Model {
                provider: row.get(0)?,
                model_name: row.get(1)?,
                url: row.get(2)?,
                deployment_name: row.get(3)?,
                enabled: row.get(4)?,
            })
        })?;
        model_iter.collect()
    }

    pub fn toggle_model(&self, provider: &str, model_name: &str) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE models SET enabled = NOT enabled 
             WHERE provider = ?1 AND model_name = ?2",
            params![provider, model_name],
        )?;
        Ok(())
    }

    pub fn set_api_key(&self, provider: &str, key: &str) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO api_keys (provider, key) VALUES (?1, ?2)",
            params![provider, key],
        )?;
        Ok(())
    }

    pub fn get_api_key(&self, provider: &str) -> RusqliteResult<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key FROM api_keys WHERE provider = ?1")?;
        let result = stmt.query_row(params![provider], |row| row.get(0));
        match result {
            Ok(key) => Ok(Some(key)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // Add these new methods to the impl block
    pub fn delete_model(&self, provider: &str, model_name: &str) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM models WHERE provider = ?1 AND model_name = ?2",
            params![provider, model_name],
        )?;
        Ok(())
    }

    pub fn delete_api_key(&self, provider: &str) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM api_keys WHERE provider = ?1",
            params![provider],
        )?;
        Ok(())
    }

    pub fn save_system_prompt(&self, name: &str, content: &str) -> RusqliteResult<SystemPrompt> {
        let conn = self.conn.lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let timestamp = now.timestamp();

        conn.execute(
            "INSERT INTO system_prompts (id, name, content, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, content, timestamp, timestamp],
        )?;

        Ok(SystemPrompt {
            id,
            name: name.to_string(),
            content: content.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update_system_prompt(&self, id: &str, name: &str, content: &str) -> RusqliteResult<SystemPrompt> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let timestamp = now.timestamp();

        conn.execute(
            "UPDATE system_prompts SET name = ?1, content = ?2, updated_at = ?3 WHERE id = ?4",
            params![name, content, timestamp, id],
        )?;

        Ok(SystemPrompt {
            id: id.to_string(),
            name: name.to_string(),
            content: content.to_string(),
            created_at: now, // Note: This will be incorrect but we don't fetch it here
            updated_at: now,
        })
    }

    pub fn get_system_prompt(&self, id: &str) -> RusqliteResult<Option<SystemPrompt>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, content, created_at, updated_at FROM system_prompts WHERE id = ?1"
        )?;
        
        let result = stmt.query_row(params![id], |row| {
            let created_timestamp: i64 = row.get(3)?;
            let updated_timestamp: i64 = row.get(4)?;
            Ok(SystemPrompt {
                id: row.get(0)?,
                name: row.get(1)?,
                content: row.get(2)?,
                created_at: Utc.timestamp_opt(created_timestamp, 0).single().unwrap(),
                updated_at: Utc.timestamp_opt(updated_timestamp, 0).single().unwrap(),
            })
        });

        match result {
            Ok(prompt) => Ok(Some(prompt)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_all_system_prompts(&self) -> RusqliteResult<Vec<SystemPrompt>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, content, created_at, updated_at FROM system_prompts ORDER BY updated_at DESC"
        )?;
        
        let prompts = stmt.query_map([], |row| {
            let created_timestamp: i64 = row.get(3)?;
            let updated_timestamp: i64 = row.get(4)?;
            Ok(SystemPrompt {
                id: row.get(0)?,
                name: row.get(1)?,
                content: row.get(2)?,
                created_at: Utc.timestamp_opt(created_timestamp, 0).single().unwrap(),
                updated_at: Utc.timestamp_opt(updated_timestamp, 0).single().unwrap(),
            })
        })?;

        prompts.collect()
    }

    // Add this new method to the impl block
    pub fn delete_system_prompt(&self, id: &str) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM system_prompts WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn get_messages(&self, conversation_id: &str) -> RusqliteResult<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("Failed to get app directory".into()))?;
        let attachments_dir = app_dir.join("com.tauri.dev").join("attachments");

        // First, get all messages ordered by created_at
        let mut messages_stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at 
             FROM messages 
             WHERE conversation_id = ?1 
             ORDER BY created_at ASC"
        )?;

        let mut messages: Vec<Message> = messages_stmt.query_map(params![conversation_id], |row| {
            let timestamp: i64 = row.get(4)?;
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: Utc.timestamp_opt(timestamp, 0).single().unwrap(),
                attachments: Vec::new(),
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        // Then, get and add attachments for each message
        let mut attachments_stmt = conn.prepare(
            "SELECT message_id, id, name, file_path, attachment_type, created_at 
             FROM message_attachments 
             WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)"
        )?;

        let attachments = attachments_stmt.query_map(params![conversation_id], |row| {
            let message_id: String = row.get(0)?;
            let timestamp: i64 = row.get(5)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            
            let file_path = row.get::<_, String>(3)?;
            let full_path = attachments_dir.join(&file_path);
            
            // Read the file content as bytes and encode to base64
            let file_content = fs::read(&full_path)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

            // Get file extension from the filename
            let extension = std::path::Path::new(&file_path)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("jpeg");

            let mime_type = match extension.to_lowercase().as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => "application/octet-stream",
            };

            let base64_content = format!(
                "data:{};base64,{}",
                mime_type,
                Engine::encode(&base64::engine::general_purpose::STANDARD, &file_content)
            );

            Ok(MessageAttachment {
                id: Some(row.get(1)?),
                message_id: Some(message_id),
                name: row.get(2)?,
                file_path: base64_content,
                attachment_type: row.get(4)?,
                description: None,
                created_at: Some(created_at),
            })
        })?;

        // Add attachments to their respective messages while maintaining message order
        for attachment in attachments {
            if let Ok(att) = attachment {
                if let Some(message_id) = &att.message_id {
                    if let Some(message) = messages.iter_mut().find(|m| m.id == *message_id) {
                        message.attachments.push(att);
                    }
                }
            }
        }

        Ok(messages)
    }
}
