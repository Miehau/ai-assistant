use super::DbOperations;
use rusqlite::{params, Result as RusqliteResult};

pub trait PreferenceOperations: DbOperations {
    fn get_preference(&self, key: &str) -> RusqliteResult<Option<String>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM user_preferences WHERE key = ?1")?;

        let result = stmt.query_row(params![key], |row| row.get(0));

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn set_preference(&self, key: &str, value: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "INSERT INTO user_preferences (key, value, updated_at)
             VALUES (?1, ?2, strftime('%s', 'now'))
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = strftime('%s', 'now')",
            params![key, value],
        )?;

        Ok(())
    }
}
