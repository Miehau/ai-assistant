use super::DbOperations;
use crate::db::models::{CreateMcpServerInput, McpServer, UpdateMcpServerInput};
use rusqlite::types::Null;
use rusqlite::{params, Result as RusqliteResult};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub trait McpServerOperations: DbOperations {
    fn create_mcp_server(&self, input: &CreateMcpServerInput) -> RusqliteResult<McpServer> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let id = Uuid::new_v4().to_string();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        conn.execute(
            "INSERT INTO mcp_servers (id, name, url, auth_type, api_key, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                input.name,
                input.url,
                input.auth_type,
                input.api_key,
                created_at,
            ],
        )?;

        Ok(McpServer {
            id,
            name: input.name.clone(),
            url: input.url.clone(),
            auth_type: input.auth_type.clone(),
            api_key: input.api_key.clone(),
            created_at,
        })
    }

    fn get_mcp_servers(&self) -> RusqliteResult<Vec<McpServer>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, auth_type, api_key, created_at FROM mcp_servers ORDER BY name",
        )?;
        let server_iter = stmt.query_map([], |row| {
            Ok(McpServer {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                auth_type: row.get(3)?,
                api_key: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        server_iter.collect()
    }

    fn get_mcp_server_by_id(&self, id: &str) -> RusqliteResult<Option<McpServer>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, auth_type, api_key, created_at FROM mcp_servers WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![id], |row| {
            Ok(McpServer {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                auth_type: row.get(3)?,
                api_key: row.get(4)?,
                created_at: row.get(5)?,
            })
        });
        match result {
            Ok(server) => Ok(Some(server)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn update_mcp_server(&self, input: &UpdateMcpServerInput) -> RusqliteResult<Option<McpServer>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

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
        if let Some(ref auth_type) = input.auth_type {
            updates.push("auth_type = ?");
            params_vec.push(Box::new(auth_type.clone()));
        }
        if let Some(ref api_key) = input.api_key {
            updates.push("api_key = ?");
            if api_key.is_empty() {
                params_vec.push(Box::new(Null));
            } else {
                params_vec.push(Box::new(api_key.clone()));
            }
        }

        if updates.is_empty() {
            drop(conn);
            return self.get_mcp_server_by_id(&input.id);
        }

        params_vec.push(Box::new(input.id.clone()));

        let sql = format!("UPDATE mcp_servers SET {} WHERE id = ?", updates.join(", "));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;

        drop(conn);
        self.get_mcp_server_by_id(&input.id)
    }

    fn delete_mcp_server(&self, id: &str) -> RusqliteResult<bool> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let rows_affected = conn.execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(rows_affected > 0)
    }
}
