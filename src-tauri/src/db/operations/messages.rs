use rusqlite::{params, Result as RusqliteResult};
use chrono::{TimeZone, Utc};
use uuid::Uuid;
use std::fs;
use base64::Engine;
use tauri::api::path;
use crate::db::models::{Message, MessageAttachment, IncomingAttachment};
use super::DbOperations;
use std::time::Instant;

pub trait MessageOperations: DbOperations {
    fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        attachments: &[IncomingAttachment],
        message_id: Option<String>,
    ) -> RusqliteResult<String> {
        // Use provided message_id if valid, otherwise generate new UUID
        let message_id = match message_id {
            Some(id) if !id.is_empty() => {
                // Validate that it's a valid UUID format (basic validation)
                // Accept both standard UUID format and our custom format for backwards compatibility
                if Uuid::parse_str(&id).is_ok() {
                    id
                } else {
                    // If not a valid UUID, generate a new one
                    Uuid::new_v4().to_string()
                }
            },
            _ => Uuid::new_v4().to_string(),
        };

        let created_at = Utc::now();
        let created_at_timestamp = created_at.timestamp();

        let binding = self.conn();
        let mut conn = binding.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![message_id, conversation_id, role, content, created_at_timestamp],
        )?;

        for attachment in attachments {
            let file_path = if attachment.attachment_type.starts_with("text/") {
                // For text attachments, store the content directly
                attachment.data.to_string()
            } else {
                // For binary attachments (images, audio), save to filesystem
                self.save_attachment_to_fs(
                    &attachment.data,
                    &attachment.name
                )?
            };

            tx.execute(
                "INSERT INTO message_attachments (
                    id, message_id, name, data, attachment_type, description, transcript, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    Uuid::new_v4().to_string(),
                    message_id,
                    attachment.name,
                    file_path,
                    attachment.attachment_type,
                    attachment.description,
                    attachment.transcript,
                    created_at_timestamp
                ],
            )?;
        }

        tx.commit()?;
        Ok(message_id)
    }

    fn get_messages(&self, conversation_id: &str) -> RusqliteResult<Vec<Message>> {
        let start_time = Instant::now();
        
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("Failed to get app directory".into()))?;
        let attachments_dir = app_dir.join("dev.michalmlak.ai_agent").join("attachments");

        println!("📁 Setup time: {:?}", start_time.elapsed());
        let messages_query_start = Instant::now();

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

        println!("📨 Messages query time: {:?}", messages_query_start.elapsed());
        let attachments_start = Instant::now();

        let mut attachments_stmt = conn.prepare(
            "SELECT message_id, id, name, data, attachment_type, created_at, description, transcript, 
             file_path, size_bytes, mime_type, thumbnail_path, updated_at 
             FROM message_attachments 
             WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)"
        )?;

        let attachments = attachments_stmt.query_map(params![conversation_id], |row| {
            let message_id: String = row.get(0)?;
            let timestamp: i64 = row.get(5)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            let attachment_type: String = row.get(4)?;
            
            let data = if attachment_type.starts_with("text/") {
                // For text attachments, use the stored content directly
                row.get::<_, String>(3)?
            } else {
                // For binary attachments, read from filesystem and encode
                let file_path: String = row.get(3)?;
                let full_path = attachments_dir.join(&file_path);
                let file_content = fs::read(&full_path)
                    .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
                let base64_data = base64::engine::general_purpose::STANDARD.encode(file_content);
                format!("data:{};base64,{}", attachment_type, base64_data)
            };
            
            // Get updated_at timestamp if available, otherwise use created_at
            let updated_at_timestamp: Option<i64> = row.get(12).ok();
            let updated_at = updated_at_timestamp
                .map(|ts| Utc.timestamp_opt(ts, 0).single().unwrap());
                
            Ok(MessageAttachment {
                id: Some(row.get(1)?),
                message_id: Some(message_id),
                name: row.get(2)?,
                data,
                attachment_url: None,
                attachment_type,
                description: row.get(6)?,
                transcript: row.get(7)?,
                created_at: Some(created_at),
                updated_at,
                file_path: row.get(8).ok(),
                size_bytes: row.get(9).ok(),
                mime_type: row.get(10).ok(),
                thumbnail_path: row.get(11).ok(),
            })
        })?;

        for attachment in attachments {
            if let Ok(att) = attachment {
                if let Some(message_id) = &att.message_id {
                    if let Some(message) = messages.iter_mut().find(|m| m.id == *message_id) {
                        message.attachments.push(att);
                    }
                }
            }
        }

        println!("📎 Total attachments processing time: {:?}", attachments_start.elapsed());
        println!("⏱️  Total get_messages time: {:?}", start_time.elapsed());

        Ok(messages)
    }

    fn save_attachment_to_fs(&self, data: &str, file_name: &str) -> RusqliteResult<String> {
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("Failed to get app directory".into()))?;
        
        let attachments_dir = app_dir.join("dev.michalmlak.ai_agent").join("attachments");
        fs::create_dir_all(&attachments_dir)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        let unique_filename = format!("{}-{}", Uuid::new_v4(), file_name);
        let file_path = attachments_dir.join(&unique_filename);
        
        let base64_data = if data.starts_with("data:") {
            data.split(",").nth(1)
                .ok_or_else(|| rusqlite::Error::InvalidParameterName("Invalid data URL format".into()))?
        } else {
            data
        };
        
        let decoded_data = base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        
        fs::write(&file_path, &decoded_data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        Ok(unique_filename)
    }
} 