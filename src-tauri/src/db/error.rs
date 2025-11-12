use rusqlite;
use rusqlite_migration;
use std::fmt;

#[derive(Debug)]
pub enum DatabaseError {
    Rusqlite(rusqlite::Error),
    Migration(rusqlite_migration::Error),
    MessageNotFound(String),
    MessageNotInTree(String),
    ConversationNotFound(String),
    BranchNotFound(String),
    ValidationError(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::Rusqlite(e) => write!(f, "Database error: {}", e),
            DatabaseError::Migration(e) => write!(f, "Migration error: {}", e),
            DatabaseError::MessageNotFound(id) => write!(f, "Message not found: {}", id),
            DatabaseError::MessageNotInTree(id) => {
                write!(f, "Message exists but is not in the message tree: {}. This conversation may need repair.", id)
            }
            DatabaseError::ConversationNotFound(id) => write!(f, "Conversation not found: {}", id),
            DatabaseError::BranchNotFound(id) => write!(f, "Branch not found: {}", id),
            DatabaseError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for DatabaseError {}

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