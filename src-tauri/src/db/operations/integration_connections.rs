use super::DbOperations;
use crate::db::models::{
    CreateIntegrationConnectionInput, IntegrationConnection, UpdateIntegrationConnectionInput,
};
use rusqlite::{params, Result as RusqliteResult};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub trait IntegrationConnectionOperations: DbOperations {
    fn create_integration_connection(
        &self,
        input: &CreateIntegrationConnectionInput,
    ) -> RusqliteResult<IntegrationConnection> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let id = Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        conn.execute(
            "INSERT INTO integration_connections (
                id,
                integration_id,
                account_label,
                status,
                auth_type,
                access_token,
                refresh_token,
                scopes,
                expires_at,
                last_error,
                last_sync_at,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id,
                input.integration_id,
                input.account_label,
                "connected",
                input.auth_type,
                input.access_token,
                input.refresh_token,
                input.scopes,
                input.expires_at,
                Option::<String>::None,
                Option::<i64>::None,
                now,
                now,
            ],
        )?;

        Ok(IntegrationConnection {
            id,
            integration_id: input.integration_id.clone(),
            account_label: input.account_label.clone(),
            status: "connected".to_string(),
            auth_type: input.auth_type.clone(),
            access_token: input.access_token.clone(),
            refresh_token: input.refresh_token.clone(),
            scopes: input.scopes.clone(),
            expires_at: input.expires_at,
            last_error: None,
            last_sync_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    fn get_integration_connections(&self) -> RusqliteResult<Vec<IntegrationConnection>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, integration_id, account_label, status, auth_type, access_token, refresh_token, scopes, expires_at, last_error, last_sync_at, created_at, updated_at
             FROM integration_connections
             ORDER BY created_at DESC"
        )?;
        let iter = stmt.query_map([], |row| {
            Ok(IntegrationConnection {
                id: row.get(0)?,
                integration_id: row.get(1)?,
                account_label: row.get(2)?,
                status: row.get(3)?,
                auth_type: row.get(4)?,
                access_token: row.get(5)?,
                refresh_token: row.get(6)?,
                scopes: row.get(7)?,
                expires_at: row.get(8)?,
                last_error: row.get(9)?,
                last_sync_at: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?;
        iter.collect()
    }

    fn get_integration_connection_by_id(
        &self,
        id: &str,
    ) -> RusqliteResult<Option<IntegrationConnection>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, integration_id, account_label, status, auth_type, access_token, refresh_token, scopes, expires_at, last_error, last_sync_at, created_at, updated_at
             FROM integration_connections WHERE id = ?1"
        )?;
        let result = stmt.query_row(params![id], |row| {
            Ok(IntegrationConnection {
                id: row.get(0)?,
                integration_id: row.get(1)?,
                account_label: row.get(2)?,
                status: row.get(3)?,
                auth_type: row.get(4)?,
                access_token: row.get(5)?,
                refresh_token: row.get(6)?,
                scopes: row.get(7)?,
                expires_at: row.get(8)?,
                last_error: row.get(9)?,
                last_sync_at: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        });
        match result {
            Ok(connection) => Ok(Some(connection)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn update_integration_connection(
        &self,
        input: &UpdateIntegrationConnectionInput,
    ) -> RusqliteResult<Option<IntegrationConnection>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut updates = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref value) = input.account_label {
            updates.push("account_label = ?");
            params_vec.push(Box::new(value.clone()));
        }
        if let Some(ref value) = input.status {
            updates.push("status = ?");
            params_vec.push(Box::new(value.clone()));
        }
        if let Some(ref value) = input.auth_type {
            updates.push("auth_type = ?");
            params_vec.push(Box::new(value.clone()));
        }
        if let Some(ref value) = input.access_token {
            updates.push("access_token = ?");
            params_vec.push(Box::new(value.clone()));
        }
        if let Some(ref value) = input.refresh_token {
            updates.push("refresh_token = ?");
            params_vec.push(Box::new(value.clone()));
        }
        if let Some(ref value) = input.scopes {
            updates.push("scopes = ?");
            params_vec.push(Box::new(value.clone()));
        }
        if let Some(value) = input.expires_at {
            updates.push("expires_at = ?");
            params_vec.push(Box::new(value));
        }
        if let Some(ref value) = input.last_error {
            updates.push("last_error = ?");
            params_vec.push(Box::new(value.clone()));
        }
        if let Some(value) = input.last_sync_at {
            updates.push("last_sync_at = ?");
            params_vec.push(Box::new(value));
        }

        if updates.is_empty() {
            drop(conn);
            return self.get_integration_connection_by_id(&input.id);
        }

        updates.push("updated_at = ?");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        params_vec.push(Box::new(now));

        params_vec.push(Box::new(input.id.clone()));

        let sql = format!(
            "UPDATE integration_connections SET {} WHERE id = ?",
            updates.join(", ")
        );

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;

        drop(conn);
        self.get_integration_connection_by_id(&input.id)
    }

    fn delete_integration_connection(&self, id: &str) -> RusqliteResult<bool> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let rows_affected = conn.execute(
            "DELETE FROM integration_connections WHERE id = ?1",
            params![id],
        )?;
        Ok(rows_affected > 0)
    }
}
