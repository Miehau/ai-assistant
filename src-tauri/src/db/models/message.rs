use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IncomingAttachment {
    pub name: String,
    pub data: String,
    pub attachment_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageAttachment {
    pub id: Option<String>,
    pub message_id: Option<String>,
    pub name: String,
    pub data: String,
    pub attachment_type: String,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub attachment_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub role: String,
    pub conversation_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<MessageAttachment>,
} 