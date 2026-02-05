use crate::db::{Db, DbOperations};
use chrono::Utc;
use uuid::Uuid;

const DEFAULT_PROMPT: &str = "You are a helpful assistant.

<rules>
1. You will always provide a helpful answer.
2. If you don't know the answer, say so. Don't make up an answer.
3. Always respond in markdown format.
</rules>

How can I help you today?";

pub fn initialize(db: &mut Db) -> Result<(), String> {
    let existing_prompt = db
        .conn()
        .lock()
        .unwrap()
        .prepare("SELECT COUNT(*) FROM system_prompts")
        .map_err(|e| e.to_string())?
        .query_row([], |row| row.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;

    if existing_prompt == 0 {
        let now = Utc::now();
        db.conn().lock().unwrap()
            .execute(
                "INSERT INTO system_prompts (id, name, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    Uuid::new_v4().to_string(),
                    "Default System Prompt",
                    DEFAULT_PROMPT,
                    now.timestamp(),
                    now.timestamp(),
                ),
            )
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
