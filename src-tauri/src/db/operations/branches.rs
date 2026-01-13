use rusqlite::{params, Result as RusqliteResult};
use chrono::{TimeZone, Utc};
use uuid::Uuid;
use std::collections::HashSet;
use crate::db::models::{
    Branch, MessageTreeNode, ConversationTree, BranchPath, BranchStats, Message, MessageTreeConsistencyCheck
};
use crate::db::DatabaseError;
use super::DbOperations;

pub trait BranchOperations: DbOperations {
    /// Create a new branch in a conversation
    fn create_branch(&self, conversation_id: &str, name: &str) -> RusqliteResult<Branch> {
        let branch_id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        let created_at_timestamp = created_at.timestamp();

        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "INSERT INTO branches (id, conversation_id, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![branch_id, conversation_id, name, created_at_timestamp],
        )?;

        Ok(Branch {
            id: branch_id,
            conversation_id: conversation_id.to_string(),
            name: name.to_string(),
            created_at,
        })
    }

    /// Create a message tree node linking a message to its parent and branch
    fn create_message_tree_node(
        &self,
        message_id: &str,
        parent_message_id: Option<&str>,
        branch_id: &str,
        is_branch_point: bool,
    ) -> RusqliteResult<MessageTreeNode> {
        let created_at = Utc::now();
        let created_at_timestamp = created_at.timestamp();

        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "INSERT INTO message_tree (message_id, parent_message_id, branch_id, branch_point, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id,
                parent_message_id,
                branch_id,
                is_branch_point as i32,
                created_at_timestamp
            ],
        )?;

        Ok(MessageTreeNode {
            message_id: message_id.to_string(),
            parent_message_id: parent_message_id.map(String::from),
            branch_id: branch_id.to_string(),
            branch_point: is_branch_point,
            created_at,
        })
    }

    /// Get all branches for a conversation
    fn get_conversation_branches(&self, conversation_id: &str) -> RusqliteResult<Vec<Branch>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, name, created_at FROM branches
             WHERE conversation_id = ?1 ORDER BY created_at ASC"
        )?;

        let branch_iter = stmt.query_map(params![conversation_id], |row| {
            let timestamp: i64 = row.get(3)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            Ok(Branch {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                name: row.get(2)?,
                created_at,
            })
        })?;

        branch_iter.collect()
    }

    /// Get all message tree nodes for a conversation
    fn get_message_tree_nodes(&self, conversation_id: &str) -> RusqliteResult<Vec<MessageTreeNode>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT mt.message_id, mt.parent_message_id, mt.branch_id, mt.branch_point, mt.created_at
             FROM message_tree mt
             JOIN messages m ON m.id = mt.message_id
             WHERE m.conversation_id = ?1
             ORDER BY mt.created_at ASC"
        )?;

        let node_iter = stmt.query_map(params![conversation_id], |row| {
            let timestamp: i64 = row.get(4)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            let branch_point: i32 = row.get(3)?;
            Ok(MessageTreeNode {
                message_id: row.get(0)?,
                parent_message_id: row.get(1)?,
                branch_id: row.get(2)?,
                branch_point: branch_point != 0,
                created_at,
            })
        })?;

        node_iter.collect()
    }

    /// Get all messages in a specific branch path
    fn get_branch_messages(&self, branch_id: &str) -> RusqliteResult<Vec<Message>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Get all message IDs in this branch
        let mut stmt = conn.prepare(
            "SELECT message_id FROM message_tree WHERE branch_id = ?1 ORDER BY created_at ASC"
        )?;

        let message_ids: Vec<String> = stmt
            .query_map(params![branch_id], |row| row.get(0))?
            .collect::<RusqliteResult<Vec<String>>>()?;

        if message_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Get messages for all IDs
        let placeholders = message_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, conversation_id, role, content, created_at FROM messages
             WHERE id IN ({}) ORDER BY created_at ASC",
            placeholders
        );

        let mut messages_stmt = conn.prepare(&query)?;
        let messages: Vec<Message> = messages_stmt
            .query_map(rusqlite::params_from_iter(message_ids.iter()), |row| {
                let timestamp: i64 = row.get(4)?;
                let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
                Ok(Message {
                    id: row.get(0)?,
                    content: row.get(3)?,
                    role: row.get(2)?,
                    conversation_id: row.get(1)?,
                    created_at,
                    attachments: Vec::new(), // Will be populated separately if needed
                    tool_executions: Vec::new(),
                })
            })?
            .collect::<RusqliteResult<Vec<Message>>>()?;

        Ok(messages)
    }

    /// Get the complete conversation tree structure
    fn get_conversation_tree(&self, conversation_id: &str) -> RusqliteResult<ConversationTree> {
        let branches = self.get_conversation_branches(conversation_id)?;
        let nodes = self.get_message_tree_nodes(conversation_id)?;

        // Get all unique message IDs from nodes
        let message_ids: HashSet<String> = nodes.iter().map(|n| n.message_id.clone()).collect();

        // Get all messages
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut messages = Vec::new();
        if !message_ids.is_empty() {
            let placeholders = message_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!(
                "SELECT id, conversation_id, role, content, created_at FROM messages
                 WHERE id IN ({}) ORDER BY created_at ASC",
                placeholders
            );

            let mut stmt = conn.prepare(&query)?;
            messages = stmt
                .query_map(rusqlite::params_from_iter(message_ids.iter()), |row| {
                    let timestamp: i64 = row.get(4)?;
                    let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
                    Ok(Message {
                        id: row.get(0)?,
                        content: row.get(3)?,
                        role: row.get(2)?,
                        conversation_id: row.get(1)?,
                        created_at,
                        attachments: Vec::new(),
                        tool_executions: Vec::new(),
                    })
                })?
                .collect::<RusqliteResult<Vec<Message>>>()?;
        }

        Ok(ConversationTree {
            conversation_id: conversation_id.to_string(),
            branches,
            nodes,
            messages,
        })
    }

    /// Get a specific branch path with its messages
    fn get_branch_path(&self, branch_id: &str) -> RusqliteResult<BranchPath> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Get branch info
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, name, created_at FROM branches WHERE id = ?1"
        )?;

        let branch = stmt.query_row(params![branch_id], |row| {
            let timestamp: i64 = row.get(3)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            Ok(Branch {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                name: row.get(2)?,
                created_at,
            })
        })?;

        // Get messages for this branch
        let messages = self.get_branch_messages(branch_id)?;

        Ok(BranchPath { branch, messages })
    }

    /// Rename a branch
    fn rename_branch(&self, branch_id: &str, new_name: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "UPDATE branches SET name = ?1 WHERE id = ?2",
            params![new_name, branch_id],
        )?;

        Ok(())
    }

    /// Delete a branch (CASCADE will handle message_tree entries)
    fn delete_branch(&self, branch_id: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "DELETE FROM branches WHERE id = ?1",
            params![branch_id],
        )?;

        Ok(())
    }

    /// Get branch statistics for a conversation
    fn get_branch_stats(&self, conversation_id: &str) -> RusqliteResult<BranchStats> {
        let branches = self.get_conversation_branches(conversation_id)?;
        let nodes = self.get_message_tree_nodes(conversation_id)?;

        let branch_points = nodes.iter().filter(|n| n.branch_point).count();

        Ok(BranchStats {
            conversation_id: conversation_id.to_string(),
            total_branches: branches.len(),
            total_messages: nodes.len(),
            branch_points,
        })
    }

    /// Get or create the main branch for a conversation (migration helper)
    fn get_or_create_main_branch(&self, conversation_id: &str) -> RusqliteResult<Branch> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Try to get existing main branch
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, name, created_at FROM branches
             WHERE conversation_id = ?1 AND name = 'Main' LIMIT 1"
        )?;

        let existing = stmt.query_row(params![conversation_id], |row| {
            let timestamp: i64 = row.get(3)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            Ok(Branch {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                name: row.get(2)?,
                created_at,
            })
        });

        // Drop stmt first to release the borrow on conn
        drop(stmt);

        match existing {
            Ok(branch) => Ok(branch),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Drop the connection before calling create_branch
                drop(conn);
                drop(binding);
                self.create_branch(conversation_id, "Main")
            }
            Err(e) => Err(e),
        }
    }

    /// Get the path of message IDs from the root to a target message in a specific branch
    fn get_message_path_to_target(
        &self,
        target_message_id: &str,
        branch_id: &str,
    ) -> RusqliteResult<Vec<String>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut path = Vec::new();
        let mut current_message_id = Some(target_message_id.to_string());

        // Walk backwards from target to root following parent links
        while let Some(msg_id) = current_message_id {
            path.push(msg_id.clone());

            // Get parent of current message in this branch
            let parent = conn.query_row(
                "SELECT parent_message_id FROM message_tree
                 WHERE message_id = ?1 AND branch_id = ?2",
                params![msg_id, branch_id],
                |row| row.get::<_, Option<String>>(0),
            );

            match parent {
                Ok(parent_id) => current_message_id = parent_id,
                Err(rusqlite::Error::QueryReturnedNoRows) => break,
                Err(e) => return Err(e),
            }
        }

        // Reverse to get root-to-target order
        path.reverse();
        Ok(path)
    }

    /// Create a new branch from a specific message, copying all messages up to that point
    fn create_branch_from_message(
        &self,
        conversation_id: &str,
        parent_message_id: &str,
        branch_name: &str,
    ) -> Result<Branch, DatabaseError> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Validation 1: Check if the message exists in the messages table
        let message_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ?1)",
                params![parent_message_id],
                |row| row.get(0),
            )?;

        if !message_exists {
            return Err(DatabaseError::MessageNotFound(parent_message_id.to_string()));
        }

        // Validation 2: Check if the conversation exists
        let conversation_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                params![conversation_id],
                |row| row.get(0),
            )?;

        if !conversation_exists {
            return Err(DatabaseError::ConversationNotFound(conversation_id.to_string()));
        }

        // Release conn for get_or_create_main_branch
        drop(conn);
        drop(binding);

        // Get the main branch
        let main_branch = self.get_or_create_main_branch(conversation_id)?;

        // Validation 3: Check if the message is in the message tree
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let message_in_tree: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM message_tree WHERE message_id = ?1 AND branch_id = ?2)",
                params![parent_message_id, main_branch.id],
                |row| row.get(0),
            )?;

        if !message_in_tree {
            drop(conn);
            drop(binding);
            return Err(DatabaseError::MessageNotInTree(parent_message_id.to_string()));
        }

        drop(conn);
        drop(binding);

        // Get all messages from root to parent_message_id in the main branch
        let message_path = self.get_message_path_to_target(parent_message_id, &main_branch.id)?;

        // Validation 4: Ensure we found a valid path
        if message_path.is_empty() {
            return Err(DatabaseError::ValidationError(
                format!("Could not find message path for message: {}", parent_message_id)
            ));
        }

        // Create the new branch
        let new_branch = self.create_branch(conversation_id, branch_name)?;

        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let created_at_timestamp = Utc::now().timestamp();

        // Copy all message tree nodes to the new branch
        for (index, message_id) in message_path.iter().enumerate() {
            let parent_id = if index > 0 {
                Some(message_path[index - 1].as_str())
            } else {
                None
            };

            conn.execute(
                "INSERT INTO message_tree (message_id, parent_message_id, branch_id, branch_point, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    message_id,
                    parent_id,
                    new_branch.id,
                    0, // Not a branch point in the new branch
                    created_at_timestamp
                ],
            )?;
        }

        // Mark the parent message as a branch point in the main branch
        conn.execute(
            "UPDATE message_tree SET branch_point = 1
             WHERE message_id = ?1 AND branch_id = ?2",
            params![parent_message_id, main_branch.id],
        )?;

        drop(conn);
        drop(binding);

        Ok(new_branch)
    }

    /// Check message tree consistency and identify orphaned messages
    fn check_message_tree_consistency(&self) -> RusqliteResult<MessageTreeConsistencyCheck> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Find messages that exist in messages table but not in message_tree
        let mut stmt = conn.prepare(
            "SELECT m.id
             FROM messages m
             WHERE NOT EXISTS (
                 SELECT 1 FROM message_tree mt WHERE mt.message_id = m.id
             )
             ORDER BY m.created_at"
        )?;

        let orphaned_messages: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<RusqliteResult<Vec<String>>>()?;

        let orphaned_count = orphaned_messages.len();
        let mut warnings = Vec::new();

        // Check for other potential issues
        // 1. Messages with parent_message_id that doesn't exist
        let parent_check: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM message_tree mt
             WHERE mt.parent_message_id IS NOT NULL
             AND NOT EXISTS (
                 SELECT 1 FROM messages m WHERE m.id = mt.parent_message_id
             )",
            [],
            |row| row.get(0),
        )?;

        if parent_check > 0 {
            warnings.push(format!("{} message tree nodes reference non-existent parent messages", parent_check));
        }

        // 2. Message tree nodes referencing non-existent messages
        let message_check: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM message_tree mt
             WHERE NOT EXISTS (
                 SELECT 1 FROM messages m WHERE m.id = mt.message_id
             )",
            [],
            |row| row.get(0),
        )?;

        if message_check > 0 {
            warnings.push(format!("{} message tree nodes reference non-existent messages", message_check));
        }

        // 3. Message tree nodes referencing non-existent branches
        let branch_check: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM message_tree mt
             WHERE NOT EXISTS (
                 SELECT 1 FROM branches b WHERE b.id = mt.branch_id
             )",
            [],
            |row| row.get(0),
        )?;

        if branch_check > 0 {
            warnings.push(format!("{} message tree nodes reference non-existent branches", branch_check));
        }

        Ok(MessageTreeConsistencyCheck {
            orphaned_messages,
            orphaned_count,
            is_consistent: orphaned_count == 0 && warnings.is_empty(),
            warnings,
        })
    }

    /// Repair message tree by adding orphaned messages to their conversation's main branch
    fn repair_message_tree(&self) -> Result<usize, DatabaseError> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Get all orphaned messages grouped by conversation
        let mut stmt = conn.prepare(
            "SELECT m.id, m.conversation_id, m.created_at
             FROM messages m
             WHERE NOT EXISTS (
                 SELECT 1 FROM message_tree mt WHERE mt.message_id = m.id
             )
             ORDER BY m.conversation_id, m.created_at"
        )?;

        let orphaned: Vec<(String, String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<RusqliteResult<Vec<_>>>()?;

        drop(stmt);

        if orphaned.is_empty() {
            return Ok(0);
        }

        let mut repaired_count = 0;

        // Group by conversation and repair
        let mut current_conversation = String::new();
        let mut conversation_messages: Vec<(String, i64)> = Vec::new();

        for (message_id, conversation_id, created_at) in orphaned.iter() {
            if current_conversation != *conversation_id {
                if !conversation_messages.is_empty() {
                    repaired_count += self.repair_conversation_messages(&current_conversation, &conversation_messages)?;
                    conversation_messages.clear();
                }
                current_conversation = conversation_id.clone();
            }
            conversation_messages.push((message_id.clone(), *created_at));
        }

        // Repair last conversation
        if !conversation_messages.is_empty() {
            repaired_count += self.repair_conversation_messages(&current_conversation, &conversation_messages)?;
        }

        Ok(repaired_count)
    }

    /// Helper function to repair messages for a single conversation
    fn repair_conversation_messages(
        &self,
        conversation_id: &str,
        messages: &[(String, i64)],
    ) -> Result<usize, DatabaseError> {
        // Get or create main branch for this conversation
        let main_branch = self.get_or_create_main_branch(conversation_id)?;

        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut count = 0;

        for (message_id, created_at) in messages {
            // Find the parent message (most recent message in the tree before this one)
            let parent_id: Option<String> = conn
                .query_row(
                    "SELECT mt.message_id
                     FROM message_tree mt
                     JOIN messages m ON m.id = mt.message_id
                     WHERE mt.branch_id = ?1
                     AND m.created_at < ?2
                     AND m.conversation_id = ?3
                     ORDER BY m.created_at DESC
                     LIMIT 1",
                    params![main_branch.id, created_at, conversation_id],
                    |row| row.get(0),
                )
                .ok();

            // Insert into message_tree
            conn.execute(
                "INSERT OR IGNORE INTO message_tree (message_id, parent_message_id, branch_id, branch_point, created_at)
                 VALUES (?1, ?2, ?3, 0, ?4)",
                params![message_id, parent_id, main_branch.id, created_at],
            )?;

            count += 1;
        }

        Ok(count)
    }
}
