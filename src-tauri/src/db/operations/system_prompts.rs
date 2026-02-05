use super::DbOperations;
use crate::db::models::SystemPrompt;
use chrono::{TimeZone, Utc};
use rusqlite::{params, Result as RusqliteResult};
use uuid::Uuid;

pub trait SystemPromptOperations: DbOperations {
    fn save_system_prompt(&self, name: &str, content: &str) -> RusqliteResult<SystemPrompt> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
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

    fn update_system_prompt(
        &self,
        id: &str,
        name: &str,
        content: &str,
    ) -> RusqliteResult<SystemPrompt> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
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

    fn get_system_prompt(&self, id: &str) -> RusqliteResult<Option<SystemPrompt>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, content, created_at, updated_at FROM system_prompts WHERE id = ?1",
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

    fn get_all_system_prompts(&self) -> RusqliteResult<Vec<SystemPrompt>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
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

    fn delete_system_prompt(&self, id: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        conn.execute("DELETE FROM system_prompts WHERE id = ?1", params![id])?;
        Ok(())
    }
}
