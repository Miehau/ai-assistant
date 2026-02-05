use super::DbOperations;
use crate::db::models::Conversation;
use chrono::{TimeZone, Utc};
use rusqlite::{params, Result as RusqliteResult};

pub trait ConversationOperations: DbOperations {
    fn get_conversations(&self) -> RusqliteResult<Vec<Conversation>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, created_at FROM conversations ORDER BY created_at DESC")?;
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

        let mut stmt =
            conn.prepare("SELECT id, name, created_at FROM conversations WHERE id = ?1")?;
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

    fn update_conversation_name(&self, conversation_id: &str, name: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        log::debug!(
            "Updating conversation name: id={}, name={}",
            conversation_id,
            name
        );

        let result = conn.execute(
            "UPDATE conversations SET name = ?1 WHERE id = ?2",
            params![name, conversation_id],
        );

        match &result {
            Ok(rows) => log::debug!("Updated {} rows", rows),
            Err(e) => log::error!("Error updating conversation name: {}", e),
        }

        result?;

        Ok(())
    }

    fn delete_conversation(&self, conversation_id: &str) -> RusqliteResult<()> {
        let binding = self.conn();

        log::debug!("Deleting conversation: id={}", conversation_id);

        // Start a transaction to ensure atomicity
        let mut binding = binding.lock().unwrap();
        let tx = binding.transaction()?;

        // First delete all message attachments for this conversation
        tx.execute(
            "DELETE FROM message_attachments WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)",
            params![conversation_id],
        )?;

        // Delete all messages for this conversation
        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![conversation_id],
        )?;

        // Finally delete the conversation itself
        let result = tx.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![conversation_id],
        )?;

        // Commit the transaction
        tx.commit()?;

        log::debug!(
            "Deleted conversation and related data, affected {} conversation rows",
            result
        );

        Ok(())
    }
}
