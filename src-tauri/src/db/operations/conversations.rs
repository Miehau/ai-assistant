use rusqlite::{params, Result as RusqliteResult};
use chrono::{TimeZone, Utc};
use crate::db::models::Conversation;
use super::DbOperations;

pub trait ConversationOperations: DbOperations {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
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

    fn get_or_create_conversation(&self, conversation_id: &str) -> RusqliteResult<Conversation> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        
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
} 