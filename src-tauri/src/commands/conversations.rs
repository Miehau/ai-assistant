use crate::db::{Conversation, Db, Message, MessageOperations, ConversationOperations, IncomingAttachment};
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn get_or_create_conversation(state: State<'_, Db>, conversation_id: Option<String>) -> Result<Conversation, String> {
    let conversation_id = conversation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    ConversationOperations::get_or_create_conversation(&*state, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_message(
    state: State<'_, Db>,
    conversation_id: String,
    role: String,
    content: String,
    attachments: Vec<IncomingAttachment>,
) -> Result<(), String> {
    MessageOperations::save_message(&*state, &conversation_id, &role, &content, &attachments)
        .map_err(|e| e.to_string())
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
pub fn update_conversation_name(state: State<'_, Db>, conversation_id: String, name: String) -> Result<(), String> {
    println!("Tauri command update_conversation_name called with id={}, name={}", conversation_id, name);
    ConversationOperations::update_conversation_name(&*state, &conversation_id, &name)
        .map_err(|e| {
            println!("Error in update_conversation_name command: {}", e);
            e.to_string()
        })
}

#[tauri::command]
pub fn delete_conversation(state: State<'_, Db>, conversation_id: String) -> Result<(), String> {
    println!("Tauri command delete_conversation called with id={}", conversation_id);
    ConversationOperations::delete_conversation(&*state, &conversation_id)
        .map_err(|e| {
            println!("Error in delete_conversation command: {}", e);
            e.to_string()
        })
}