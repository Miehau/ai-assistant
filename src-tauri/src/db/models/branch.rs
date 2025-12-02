use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::message::Message;

/// Represents a branch in a conversation
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Branch {
    pub id: String,
    pub conversation_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Represents a node in the message tree structure
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct MessageTreeNode {
    pub message_id: String,
    pub parent_message_id: Option<String>,
    pub branch_id: String,
    pub branch_point: bool,
    pub created_at: DateTime<Utc>,
}

/// Complete tree structure for a conversation with messages
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct ConversationTree {
    pub conversation_id: String,
    pub branches: Vec<Branch>,
    pub nodes: Vec<MessageTreeNode>,
    pub messages: Vec<Message>,
}

/// Represents a single branch path with its messages
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct BranchPath {
    pub branch: Branch,
    pub messages: Vec<Message>,
}

/// Statistics about branches in a conversation
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct BranchStats {
    pub conversation_id: String,
    pub total_branches: usize,
    pub total_messages: usize,
    pub branch_points: usize,
}

/// Results of a database consistency check for message tree
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct MessageTreeConsistencyCheck {
    /// Messages that exist in messages table but not in message_tree
    pub orphaned_messages: Vec<String>,
    /// Number of orphaned messages
    pub orphaned_count: usize,
    /// Whether the database is consistent
    pub is_consistent: bool,
    /// Any additional warnings or issues found
    pub warnings: Vec<String>,
}
