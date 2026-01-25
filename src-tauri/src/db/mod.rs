// src/db/mod.rs
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::sync::{Arc, Mutex};

mod error;
mod models;
mod operations;
#[cfg(test)]
mod tests;

pub use error::DatabaseError;
pub use models::*;
pub use operations::*;

#[derive(Clone)]
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
impl CustomBackendOperations for Db {}
impl PreferenceOperations for Db {}
impl AgentSessionOperations for Db {}

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
            // Agent tool executions table
            M::up("CREATE TABLE IF NOT EXISTS message_tool_executions (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                parameters TEXT NOT NULL,
                result TEXT NOT NULL,
                success INTEGER NOT NULL,
                duration INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                error TEXT,
                iteration_number INTEGER NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );"),
            M::up("CREATE INDEX IF NOT EXISTS idx_tool_executions_message ON message_tool_executions(message_id);"),
            // Agent thinking table
            M::up("CREATE TABLE IF NOT EXISTS message_agent_thinking (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                stage TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                iteration_number INTEGER NOT NULL,
                metadata TEXT,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );"),
            M::up("CREATE INDEX IF NOT EXISTS idx_agent_thinking_message ON message_agent_thinking(message_id);"),
            // Custom backends table for user-defined API endpoints
            M::up("CREATE TABLE IF NOT EXISTS custom_backends (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                url TEXT NOT NULL,
                api_key TEXT,
                created_at INTEGER NOT NULL
            );"),
            // Add custom_backend_id column to models table
            M::up("ALTER TABLE models ADD COLUMN custom_backend_id TEXT;"),
            // User preferences table for storing app settings
            M::up("CREATE TABLE IF NOT EXISTS user_preferences (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER DEFAULT (strftime('%s', 'now'))
            );"),
            // Agent session orchestration tables
            M::up("CREATE TABLE IF NOT EXISTS agent_sessions (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                phase TEXT NOT NULL,
                phase_data TEXT NOT NULL,
                config TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                completed_at INTEGER,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );"),
            M::up("CREATE INDEX IF NOT EXISTS idx_agent_sessions_conversation ON agent_sessions(conversation_id);"),
            M::up("CREATE TABLE IF NOT EXISTS agent_plans (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES agent_sessions(id),
                goal TEXT NOT NULL,
                assumptions TEXT NOT NULL,
                revision_number INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
            );"),
            M::up("CREATE INDEX IF NOT EXISTS idx_agent_plans_session ON agent_plans(session_id);"),
            M::up("CREATE TABLE IF NOT EXISTS agent_plan_steps (
                id TEXT PRIMARY KEY,
                plan_id TEXT NOT NULL REFERENCES agent_plans(id),
                sequence INTEGER NOT NULL,
                description TEXT NOT NULL,
                expected_outcome TEXT NOT NULL,
                action_type TEXT NOT NULL,
                action_data TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at INTEGER NOT NULL,
                FOREIGN KEY (plan_id) REFERENCES agent_plans(id) ON DELETE CASCADE
            );"),
            M::up("CREATE INDEX IF NOT EXISTS idx_agent_plan_steps_plan ON agent_plan_steps(plan_id);"),
            M::up("CREATE TABLE IF NOT EXISTS agent_step_results (
                id TEXT PRIMARY KEY,
                step_id TEXT NOT NULL REFERENCES agent_plan_steps(id),
                session_id TEXT NOT NULL REFERENCES agent_sessions(id),
                success INTEGER NOT NULL,
                output TEXT,
                error TEXT,
                duration_ms INTEGER NOT NULL,
                completed_at INTEGER NOT NULL,
                FOREIGN KEY (step_id) REFERENCES agent_plan_steps(id) ON DELETE CASCADE,
                FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
            );"),
            M::up("CREATE INDEX IF NOT EXISTS idx_agent_step_results_step ON agent_step_results(step_id);"),
            M::up("CREATE TABLE IF NOT EXISTS agent_step_approvals (
                id TEXT PRIMARY KEY,
                step_id TEXT NOT NULL REFERENCES agent_plan_steps(id),
                decision TEXT NOT NULL,
                auto_approve_reason TEXT,
                feedback TEXT,
                decided_at INTEGER NOT NULL,
                FOREIGN KEY (step_id) REFERENCES agent_plan_steps(id) ON DELETE CASCADE
            );"),
            M::up("CREATE INDEX IF NOT EXISTS idx_agent_step_approvals_step ON agent_step_approvals(step_id);"),
        ]);

        let mut conn = self.conn.lock().unwrap();
        migrations.to_latest(&mut *conn)?;
        Ok(())
    }
}
