use rusqlite::{params, Result as RusqliteResult};
use crate::db::models::Model;
use super::DbOperations;

pub trait ModelOperations: DbOperations {
    fn add_model(&self, model: &Model) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        conn.execute(
            "INSERT INTO models (provider, model_name, url, deployment_name, enabled, custom_backend_id)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![
                model.provider,
                model.model_name,
                model.url,
                model.deployment_name,
                model.custom_backend_id,
            ],
        )?;
        Ok(())
    }

    fn get_models(&self) -> RusqliteResult<Vec<Model>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT provider, model_name, url, deployment_name, enabled, custom_backend_id FROM models"
        )?;
        let model_iter = stmt.query_map([], |row| {
            Ok(Model {
                provider: row.get(0)?,
                model_name: row.get(1)?,
                url: row.get(2)?,
                deployment_name: row.get(3)?,
                enabled: row.get(4)?,
                custom_backend_id: row.get(5)?,
            })
        })?;
        model_iter.collect()
    }

    fn toggle_model(&self, provider: &str, model_name: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        conn.execute(
            "UPDATE models SET enabled = NOT enabled 
             WHERE provider = ?1 AND model_name = ?2",
            params![provider, model_name],
        )?;
        Ok(())
    }

    fn delete_model(&self, provider: &str, model_name: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        conn.execute(
            "DELETE FROM models WHERE provider = ?1 AND model_name = ?2",
            params![provider, model_name],
        )?;
        Ok(())
    }

    fn set_api_key(&self, provider: &str, key: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO api_keys (provider, key) VALUES (?1, ?2)",
            params![provider, key],
        )?;
        Ok(())
    }

    fn get_api_key(&self, provider: &str) -> RusqliteResult<Option<String>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key FROM api_keys WHERE provider = ?1")?;
        let result = stmt.query_row(params![provider], |row| row.get(0));
        match result {
            Ok(key) => Ok(Some(key)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn delete_api_key(&self, provider: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        conn.execute(
            "DELETE FROM api_keys WHERE provider = ?1",
            params![provider],
        )?;
        Ok(())
    }
} 