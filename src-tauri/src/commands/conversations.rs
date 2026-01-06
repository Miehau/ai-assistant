use crate::db::{Conversation, Db, Message, MessageOperations, ConversationOperations, IncomingAttachment};
use crate::events::{
    AgentEvent,
    EventBus,
    EVENT_CONVERSATION_DELETED,
    EVENT_CONVERSATION_UPDATED,
    EVENT_MESSAGE_SAVED,
};
use chrono::Utc;
use serde_json::json;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn get_or_create_conversation(state: State<'_, Db>, conversation_id: Option<String>) -> Result<Conversation, String> {
    let conversation_id = conversation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    ConversationOperations::get_or_create_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_message(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    conversation_id: String,
    role: String,
    content: String,
    attachments: Vec<IncomingAttachment>,
    message_id: Option<String>,
) -> Result<String, String> {
    let saved_message_id = MessageOperations::save_message(
        &*state,
        &conversation_id,
        &role,
        &content,
        &attachments,
        message_id,
    )
    .map_err(|e| e.to_string())?;

    let timestamp_ms = Utc::now().timestamp_millis();
    event_bus.publish(AgentEvent::new_with_timestamp(
        EVENT_MESSAGE_SAVED,
        json!({
            "conversation_id": conversation_id,
            "message_id": saved_message_id,
            "role": role,
            "content": content,
            "attachments": attachments,
            "timestamp_ms": timestamp_ms
        }),
        timestamp_ms,
    ));

    Ok(saved_message_id)
}

#[tauri::command]
pub fn get_conversation_history(state: State<'_, Db>, conversation_id: String) -> Result<Vec<Message>, String> {
    MessageOperations::get_messages(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversations(state: State<'_, Db>) -> Result<Vec<Conversation>, String> {
    ConversationOperations::get_conversations(&*state)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_conversation_name(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    conversation_id: String,
    name: String,
) -> Result<(), String> {
    println!("Tauri command update_conversation_name called with id={}, name={}", conversation_id, name);
    ConversationOperations::update_conversation_name(&*state, &conversation_id, &name)
        .map_err(|e| {
            println!("Error in update_conversation_name command: {}", e);
            e.to_string()
        })?;

    let timestamp_ms = Utc::now().timestamp_millis();
    event_bus.publish(AgentEvent::new_with_timestamp(
        EVENT_CONVERSATION_UPDATED,
        json!({
            "conversation_id": conversation_id,
            "name": name,
            "timestamp_ms": timestamp_ms
        }),
        timestamp_ms,
    ));

    Ok(())
}

#[tauri::command]
pub fn delete_conversation(
    state: State<'_, Db>,
    event_bus: State<'_, EventBus>,
    conversation_id: String,
) -> Result<(), String> {
    println!("Tauri command delete_conversation called with id={}", conversation_id);
    ConversationOperations::delete_conversation(&*state, &conversation_id)
        .map_err(|e| {
            println!("Error in delete_conversation command: {}", e);
            e.to_string()
        })?;

    let timestamp_ms = Utc::now().timestamp_millis();
    event_bus.publish(AgentEvent::new_with_timestamp(
        EVENT_CONVERSATION_DELETED,
        json!({
            "conversation_id": conversation_id,
            "timestamp_ms": timestamp_ms
        }),
        timestamp_ms,
    ));

    Ok(())
}
