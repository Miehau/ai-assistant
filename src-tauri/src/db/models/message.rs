use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IncomingAttachment {
    pub name: String,
    pub data: String,
    pub attachment_type: String,
    pub description: Option<String>,
    pub transcript: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageAttachment {
    pub id: Option<String>,
    pub message_id: Option<String>,
    pub name: String,
    pub data: String,
    pub attachment_type: String,
    pub description: Option<String>,
    pub transcript: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub attachment_url: Option<String>,
    // New fields for improved file handling
    pub file_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    pub thumbnail_path: Option<String>,
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