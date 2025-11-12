use crate::db::{Db, UsageOperations, SaveMessageUsageInput, MessageUsage, ConversationUsageSummary, UsageStatistics};
use tauri::State;
use chrono::{TimeZone, Utc};

#[tauri::command]
pub fn save_message_usage(
    state: State<'_, Db>,
    input: SaveMessageUsageInput
) -> Result<MessageUsage, String> {
    UsageOperations::save_message_usage(&*state, input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_conversation_usage(
    state: State<'_, Db>,
    conversation_id: String
) -> Result<ConversationUsageSummary, String> {
    UsageOperations::update_conversation_usage(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation_usage(
    state: State<'_, Db>,
    conversation_id: String
) -> Result<Option<ConversationUsageSummary>, String> {
    UsageOperations::get_conversation_usage(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_usage_statistics(
    state: State<'_, Db>,
    start_date: Option<i64>,
    end_date: Option<i64>
) -> Result<UsageStatistics, String> {
    // Convert Option<i64> timestamps to Option<DateTime<Utc>>
    let start_date_time = start_date.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
    let end_date_time = end_date.and_then(|ts| Utc.timestamp_opt(ts, 0).single());

    UsageOperations::get_usage_statistics(&*state, start_date_time, end_date_time)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_message_usage(
    state: State<'_, Db>,
    message_id: String
) -> Result<Option<MessageUsage>, String> {
    UsageOperations::get_message_usage(&*state, &message_id)
        .map_err(|e| e.to_string())
}
