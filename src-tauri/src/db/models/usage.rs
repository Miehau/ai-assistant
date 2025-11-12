use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Token usage data for a single message
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageUsage {
    pub id: String,
    pub message_id: String,
    pub model_name: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub estimated_cost: f64,
    pub created_at: DateTime<Utc>,
}

/// Aggregated usage summary for a conversation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConversationUsageSummary {
    pub conversation_id: String,
    pub total_prompt_tokens: i32,
    pub total_completion_tokens: i32,
    pub total_tokens: i32,
    pub total_cost: f64,
    pub message_count: i32,
    pub last_updated: DateTime<Utc>,
}

/// Input structure for saving message usage
#[derive(Debug, Deserialize)]
pub struct SaveMessageUsageInput {
    pub message_id: String,
    pub model_name: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub estimated_cost: f64,
}

/// Usage statistics for a date range
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsageStatistics {
    pub total_messages: i32,
    pub total_tokens: i32,
    pub total_cost: f64,
    pub by_model: Vec<ModelUsage>,
    pub by_date: Vec<DailyUsage>,
}

/// Usage breakdown by model
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelUsage {
    pub model_name: String,
    pub message_count: i32,
    pub total_tokens: i32,
    pub total_cost: f64,
}

/// Usage breakdown by date
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DailyUsage {
    pub date: String,
    pub message_count: i32,
    pub total_tokens: i32,
    pub total_cost: f64,
}
