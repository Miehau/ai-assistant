use super::DbOperations;
use crate::db::models::{CreateCustomBackendInput, CustomBackend, UpdateCustomBackendInput};
use rusqlite::{params, Result as RusqliteResult};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub trait CustomBackendOperations: DbOperations {
    fn create_custom_backend(
        &self,
        input: &CreateCustomBackendInput,
    ) -> RusqliteResult<CustomBackend> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let id = Uuid::new_v4().to_string();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        conn.execute(
            "INSERT INTO custom_backends (id, name, url, api_key, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, input.name, input.url, input.api_key, created_at,],
        )?;

        Ok(CustomBackend {
            id,
            name: input.name.clone(),
            url: input.url.clone(),
            api_key: input.api_key.clone(),
            created_at,
        })
    }

    fn get_custom_backends(&self) -> RusqliteResult<Vec<CustomBackend>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, api_key, created_at FROM custom_backends ORDER BY name",
        )?;
        let backend_iter = stmt.query_map([], |row| {
            Ok(CustomBackend {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                api_key: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        backend_iter.collect()
    }

    fn get_custom_backend_by_id(&self, id: &str) -> RusqliteResult<Option<CustomBackend>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, api_key, created_at FROM custom_backends WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![id], |row| {
            Ok(CustomBackend {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                api_key: row.get(3)?,
                created_at: row.get(4)?,
            })
        });
        match result {
            Ok(backend) => Ok(Some(backend)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn update_custom_backend(
        &self,
        input: &UpdateCustomBackendInput,
    ) -> RusqliteResult<Option<CustomBackend>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Build dynamic update query
        let mut updates = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref name) = input.name {
            updates.push("name = ?");
            params_vec.push(Box::new(name.clone()));
        }
        if let Some(ref url) = input.url {
            updates.push("url = ?");
            params_vec.push(Box::new(url.clone()));
        }
        if let Some(ref api_key) = input.api_key {
            updates.push("api_key = ?");
            params_vec.push(Box::new(api_key.clone()));
        }

        if updates.is_empty() {
            // No updates, just return the existing backend
            drop(conn);
            return self.get_custom_backend_by_id(&input.id);
        }

        params_vec.push(Box::new(input.id.clone()));

        let sql = format!(
            "UPDATE custom_backends SET {} WHERE id = ?",
            updates.join(", ")
        );

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;

        drop(conn);
        self.get_custom_backend_by_id(&input.id)
    }

    fn delete_custom_backend(&self, id: &str) -> RusqliteResult<bool> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let rows_affected =
            conn.execute("DELETE FROM custom_backends WHERE id = ?1", params![id])?;
        Ok(rows_affected > 0)
    }
}
