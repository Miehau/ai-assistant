// src/db/mod.rs
use rusqlite::{Connection, Result as RusqliteResult};
use rusqlite_migration::{Migrations, M};
use std::sync::{Arc, Mutex};
use rusqlite::params;

// Define a custom error type
#[derive(Debug)]
pub enum DatabaseError {
    Rusqlite(rusqlite::Error),
    Migration(rusqlite_migration::Error),
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(err: rusqlite::Error) -> Self {
        DatabaseError::Rusqlite(err)
    }
}

impl From<rusqlite_migration::Error> for DatabaseError {
    fn from(err: rusqlite_migration::Error) -> Self {
        DatabaseError::Migration(err)
    }
}

pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn new(db_path: &str) -> Result<Self, DatabaseError> {
        let conn = Connection::open(db_path)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)), })
    }

    pub fn run_migrations(&mut self) -> Result<(), DatabaseError> {
        // Define your migrations
        let migrations = Migrations::new(vec![
            M::up("CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE
            );"),
            M::up("CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );"),
        ]);

        let mut conn = self.conn.lock().unwrap();
        migrations.to_latest(&mut *conn)?;
        Ok(())
    }

    pub fn save_memory(&self, content: &str) -> RusqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memories (content) VALUES (?1)",
            params![content],
        )?;
        Ok(())
    }
}