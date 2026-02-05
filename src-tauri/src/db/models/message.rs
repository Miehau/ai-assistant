use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct IncomingAttachment {
    pub name: String,
    pub data: String,
    pub attachment_type: String,
    pub description: Option<String>,
    pub transcript: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
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

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub role: String,
    pub conversation_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<MessageAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_executions: Vec<MessageToolExecution>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct MessageToolExecution {
    pub id: String,
    pub message_id: String,
    pub tool_name: String,
    pub parameters: Value,
    pub result: Value,
    pub success: bool,
    pub duration_ms: i64,
    pub timestamp_ms: i64,
    pub error: Option<String>,
    pub iteration_number: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct MessageToolExecutionInput {
    pub id: String,
    pub message_id: String,
    pub tool_name: String,
    pub parameters: Value,
    pub result: Value,
    pub success: bool,
    pub duration_ms: i64,
    pub timestamp_ms: i64,
    pub error: Option<String>,
    pub iteration_number: i64,
}
