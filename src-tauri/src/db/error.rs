use rusqlite;
use rusqlite_migration;

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