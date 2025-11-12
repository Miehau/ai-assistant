// src/db/mod.rs
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::sync::{Arc, Mutex};

mod error;
mod models;
mod operations;

pub use error::DatabaseError;
pub use models::*;
pub use operations::*;

pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl DbOperations for Db {
    fn conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }
}

impl MessageOperations for Db {}
impl ConversationOperations for Db {}
impl ModelOperations for Db {}
impl SystemPromptOperations for Db {}
impl UsageOperations for Db {}
impl BranchOperations for Db {}

impl Db {
    pub fn new(db_path: &str) -> Result<Self, DatabaseError> {
        let conn = Connection::open(db_path)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn run_migrations(&mut self) -> Result<(), DatabaseError> {
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
            M::up("CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );"),
            M::up("CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                role TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            );"),
            M::up("CREATE TABLE IF NOT EXISTS models (
                provider TEXT NOT NULL,
                model_name TEXT NOT NULL,
                url TEXT,
                deployment_name TEXT,
                enabled BOOLEAN NOT NULL DEFAULT 0,
                PRIMARY KEY (provider, model_name)
            );"),
            M::up("CREATE TABLE IF NOT EXISTS api_keys (
                provider TEXT PRIMARY KEY,
                key TEXT NOT NULL
            );"),
            M::up("CREATE TABLE IF NOT EXISTS system_prompts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT 'Untitled',
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );"),
            M::up("CREATE TABLE IF NOT EXISTS message_attachments (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                name TEXT NOT NULL,
                data TEXT NOT NULL,
                attachment_type TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id)
            );"),
            M::up(
                "ALTER TABLE message_attachments ADD COLUMN transcript TEXT;",
            ),
            M::up(
                "ALTER TABLE message_attachments ADD COLUMN description TEXT;",
            ),
            // New migrations for file attachment improvements
            M::up(
                "ALTER TABLE message_attachments ADD COLUMN file_path TEXT;",
            ),
            M::up(
                "ALTER TABLE message_attachments ADD COLUMN size_bytes INTEGER;",
            ),
            M::up(
                "ALTER TABLE message_attachments ADD COLUMN mime_type TEXT;",
            ),
            M::up(
                "ALTER TABLE message_attachments ADD COLUMN thumbnail_path TEXT;",
            ),
            M::up(
                "ALTER TABLE message_attachments ADD COLUMN updated_at INTEGER;",
            ),
            // Token usage tracking tables
            M::up("CREATE TABLE IF NOT EXISTS message_usage (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                model_name TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL,
                completion_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                estimated_cost REAL NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );"),
            M::up("CREATE TABLE IF NOT EXISTS conversation_usage_summary (
                conversation_id TEXT PRIMARY KEY,
                total_prompt_tokens INTEGER DEFAULT 0,
                total_completion_tokens INTEGER DEFAULT 0,
                total_tokens INTEGER DEFAULT 0,
                total_cost REAL DEFAULT 0.0,
                message_count INTEGER DEFAULT 0,
                last_updated INTEGER NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );"),
            // Index for faster queries
            M::up("CREATE INDEX IF NOT EXISTS idx_message_usage_message_id ON message_usage(message_id);"),
            M::up("CREATE INDEX IF NOT EXISTS idx_message_usage_created_at ON message_usage(created_at);"),
            // Conversation branching tables
            M::up("CREATE TABLE IF NOT EXISTS branches (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );"),
            M::up("CREATE TABLE IF NOT EXISTS message_tree (
                message_id TEXT PRIMARY KEY,
                parent_message_id TEXT,
                branch_id TEXT NOT NULL,
                branch_point BOOLEAN NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
                FOREIGN KEY (parent_message_id) REFERENCES messages(id) ON DELETE CASCADE,
                FOREIGN KEY (branch_id) REFERENCES branches(id) ON DELETE CASCADE
            );"),
            // Indexes for branch queries
            M::up("CREATE INDEX IF NOT EXISTS idx_branches_conversation_id ON branches(conversation_id);"),
            M::up("CREATE INDEX IF NOT EXISTS idx_message_tree_parent ON message_tree(parent_message_id);"),
            M::up("CREATE INDEX IF NOT EXISTS idx_message_tree_branch ON message_tree(branch_id);"),
            // Fix message_tree to support messages in multiple branches (composite primary key)
            M::up("
                -- Create new message_tree with composite primary key
                CREATE TABLE IF NOT EXISTS message_tree_new (
                    message_id TEXT NOT NULL,
                    parent_message_id TEXT,
                    branch_id TEXT NOT NULL,
                    branch_point BOOLEAN NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY (message_id, branch_id),
                    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
                    FOREIGN KEY (parent_message_id) REFERENCES messages(id) ON DELETE CASCADE,
                    FOREIGN KEY (branch_id) REFERENCES branches(id) ON DELETE CASCADE
                );

                -- Copy existing data
                INSERT INTO message_tree_new (message_id, parent_message_id, branch_id, branch_point, created_at)
                SELECT message_id, parent_message_id, branch_id, branch_point, created_at FROM message_tree;

                -- Drop old table
                DROP TABLE message_tree;

                -- Rename new table
                ALTER TABLE message_tree_new RENAME TO message_tree;

                -- Recreate indexes
                CREATE INDEX IF NOT EXISTS idx_message_tree_parent ON message_tree(parent_message_id);
                CREATE INDEX IF NOT EXISTS idx_message_tree_branch ON message_tree(branch_id);
            "),
            // Populate message_tree for existing messages that lack entries
            M::up("
                -- For each conversation, create a Main branch if it doesn't exist,
                -- then add all messages without message_tree entries

                -- Step 1: Create Main branch for conversations that don't have one
                INSERT OR IGNORE INTO branches (id, conversation_id, name, created_at)
                SELECT
                    'main-' || c.id,
                    c.id,
                    'Main',
                    c.created_at
                FROM conversations c
                WHERE NOT EXISTS (
                    SELECT 1 FROM branches b
                    WHERE b.conversation_id = c.id AND b.name = 'Main'
                );

                -- Step 2: Find messages without message_tree entries and add them
                INSERT OR IGNORE INTO message_tree (message_id, parent_message_id, branch_id, branch_point, created_at)
                SELECT
                    m.id as message_id,
                    (
                        -- Find the previous message in the same conversation as parent
                        SELECT m2.id
                        FROM messages m2
                        WHERE m2.conversation_id = m.conversation_id
                            AND m2.created_at < m.created_at
                        ORDER BY m2.created_at DESC
                        LIMIT 1
                    ) as parent_message_id,
                    -- Use the Main branch for this conversation
                    COALESCE(
                        (SELECT id FROM branches WHERE conversation_id = m.conversation_id AND name = 'Main' LIMIT 1),
                        'main-' || m.conversation_id
                    ) as branch_id,
                    0 as branch_point,
                    m.created_at
                FROM messages m
                WHERE NOT EXISTS (
                    SELECT 1 FROM message_tree mt WHERE mt.message_id = m.id
                );
            "),
        ]);

        let mut conn = self.conn.lock().unwrap();
        migrations.to_latest(&mut *conn)?;
        Ok(())
    }
}
