use rusqlite::{params, Result as RusqliteResult};
use chrono::{TimeZone, Utc};
use uuid::Uuid;
use std::fs;
use base64::Engine;
use tauri::api::path;
use crate::db::models::{Message, MessageAttachment, IncomingAttachment};
use super::DbOperations;

pub trait MessageOperations: DbOperations {
    fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        attachments: &[IncomingAttachment],
    ) -> RusqliteResult<()> {
        let message_id = Uuid::new_v4().to_string();
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
            let file_path: String = self.save_attachment_to_fs(
                &attachment.data,
                &attachment.name
            )?;

            tx.execute(
                "INSERT INTO message_attachments (id, message_id, name, data, attachment_type, created_at)
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

    fn get_messages(&self, conversation_id: &str) -> RusqliteResult<Vec<Message>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("Failed to get app directory".into()))?;
        let attachments_dir = app_dir.join("com.tauri.dev").join("attachments");

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

        let mut attachments_stmt = conn.prepare(
            "SELECT message_id, id, name, data, attachment_type, created_at 
             FROM message_attachments 
             WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)"
        )?;

        let attachments = attachments_stmt.query_map(params![conversation_id], |row| {
            let message_id: String = row.get(0)?;
            let timestamp: i64 = row.get(5)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            
            let file_path = row.get::<_, String>(3)?;
            let full_path = attachments_dir.join(&file_path);
            
            let file_content = fs::read(&full_path)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

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
                data: base64_content,
                attachment_type: row.get(4)?,
                description: None,
                created_at: Some(created_at),
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

        Ok(messages)
    }

    fn save_attachment_to_fs(&self, data: &str, file_name: &str) -> RusqliteResult<String> {
        let app_dir = path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("Failed to get app directory".into()))?;
        
        let attachments_dir = app_dir.join("com.tauri.dev").join("attachments");
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
        
        fs::write(&file_path, decoded_data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        Ok(unique_filename)
    }
} 