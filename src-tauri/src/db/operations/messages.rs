use super::DbOperations;
use crate::db::models::{
    IncomingAttachment, Message, MessageAgentThinking, MessageAgentThinkingInput, MessageAttachment,
    MessageToolExecution, MessageToolExecutionInput,
};
use base64::Engine;
use chrono::{TimeZone, Utc};
use rusqlite::{params, params_from_iter, OptionalExtension, Result as RusqliteResult};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::api::path;
use uuid::Uuid;

const ATTACHMENT_BASE64_CACHE_MAX_ENTRIES: usize = 64;

struct AttachmentBase64Cache {
    order: VecDeque<String>,
    entries: HashMap<String, String>,
}

impl AttachmentBase64Cache {
    fn new() -> Self {
        Self {
            order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<String> {
        if let Some(value) = self.entries.get(key).cloned() {
            if let Some(pos) = self.order.iter().position(|entry| entry == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.to_string());
            Some(value)
        } else {
            None
        }
    }

    fn insert(&mut self, key: String, value: String) {
        if self.entries.contains_key(&key) {
            self.order.retain(|entry| entry != &key);
        }
        self.entries.insert(key.clone(), value);
        self.order.push_back(key);
        while self.order.len() > ATTACHMENT_BASE64_CACHE_MAX_ENTRIES {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

static ATTACHMENT_BASE64_CACHE: OnceLock<Mutex<AttachmentBase64Cache>> = OnceLock::new();

fn attachment_cache() -> &'static Mutex<AttachmentBase64Cache> {
    ATTACHMENT_BASE64_CACHE.get_or_init(|| Mutex::new(AttachmentBase64Cache::new()))
}

#[derive(Debug, Clone)]
pub struct PromptHistoryOptions {
    pub max_messages: Option<usize>,
    pub max_total_chars: Option<usize>,
    pub attachments_full_window: usize,
    pub include_attachments: bool,
    pub include_tool_executions: bool,
}

impl PromptHistoryOptions {
    pub fn new() -> Self {
        Self {
            max_messages: None,
            max_total_chars: None,
            attachments_full_window: 0,
            include_attachments: true,
            include_tool_executions: true,
        }
    }
}

fn attachments_dir() -> RusqliteResult<PathBuf> {
    let app_dir = path::app_data_dir(&tauri::Config::default()).ok_or_else(|| {
        rusqlite::Error::InvalidParameterName("Failed to get app directory".into())
    })?;
    Ok(app_dir.join("dev.michalmlak.ai_agent").join("attachments"))
}

fn read_attachment_base64_cached(
    attachment_id: &str,
    attachment_type: &str,
    file_path: &str,
    attachments_dir: &PathBuf,
) -> RusqliteResult<String> {
    if let Ok(mut cache) = attachment_cache().lock() {
        if let Some(cached) = cache.get(attachment_id) {
            return Ok(cached);
        }
    }

    let full_path = attachments_dir.join(file_path);
    let file_content = fs::read(&full_path)
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let base64_data = base64::engine::general_purpose::STANDARD.encode(file_content);
    let data_url = format!("data:{};base64,{}", attachment_type, base64_data);

    if let Ok(mut cache) = attachment_cache().lock() {
        cache.insert(attachment_id.to_string(), data_url.clone());
    }

    Ok(data_url)
}

fn trim_messages_by_char_budget(
    mut messages: Vec<Message>,
    max_total_chars: Option<usize>,
) -> Vec<Message> {
    let Some(limit) = max_total_chars else {
        return messages;
    };

    let mut total_chars: usize = messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum();

    while total_chars > limit && messages.len() > 1 {
        if let Some(first) = messages.first() {
            total_chars = total_chars.saturating_sub(first.content.chars().count());
        }
        messages.remove(0);
    }

    messages
}

fn build_in_clause(placeholders: usize) -> String {
    std::iter::repeat("?")
        .take(placeholders)
        .collect::<Vec<_>>()
        .join(", ")
}

pub trait MessageOperations: DbOperations {
    fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        attachments: &[IncomingAttachment],
        message_id: Option<String>,
    ) -> RusqliteResult<String> {
        // Use provided message_id if valid, otherwise generate new UUID
        let message_id = match message_id {
            Some(id) if !id.is_empty() => {
                // Validate that it's a valid UUID format (basic validation)
                // Accept both standard UUID format and our custom format for backwards compatibility
                if Uuid::parse_str(&id).is_ok() {
                    id
                } else {
                    // If not a valid UUID, generate a new one
                    Uuid::new_v4().to_string()
                }
            }
            _ => Uuid::new_v4().to_string(),
        };

        let created_at = Utc::now();
        let created_at_timestamp = created_at.timestamp();

        let binding = self.conn();
        let mut conn = binding.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id,
                conversation_id,
                role,
                content,
                created_at_timestamp
            ],
        )?;

        for attachment in attachments {
            let file_path = if attachment.attachment_type.starts_with("text/") {
                // For text attachments, store the content directly
                attachment.data.to_string()
            } else {
                // For binary attachments (images, audio), save to filesystem
                self.save_attachment_to_fs(&attachment.data, &attachment.name)?
            };

            tx.execute(
                "INSERT INTO message_attachments (
                    id, message_id, name, data, attachment_type, description, transcript, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    Uuid::new_v4().to_string(),
                    message_id,
                    attachment.name,
                    file_path,
                    attachment.attachment_type,
                    attachment.description,
                    attachment.transcript,
                    created_at_timestamp
                ],
            )?;
        }

        tx.commit()?;
        Ok(message_id)
    }

    fn save_tool_execution(
        &self,
        input: MessageToolExecutionInput,
    ) -> RusqliteResult<MessageToolExecution> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "INSERT INTO message_tool_executions (
                id, message_id, tool_name, parameters, result, success, duration, timestamp, error, iteration_number,
                session_id, parent_session_id, is_sub_agent
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                input.id,
                input.message_id,
                input.tool_name,
                input.parameters.to_string(),
                input.result.to_string(),
                input.success,
                input.duration_ms,
                input.timestamp_ms,
                input.error,
                input.iteration_number,
                input.session_id,
                input.parent_session_id,
                input.is_sub_agent,
            ],
        )?;

        Ok(MessageToolExecution {
            id: input.id,
            message_id: input.message_id,
            tool_name: input.tool_name,
            parameters: input.parameters,
            result: input.result,
            success: input.success,
            duration_ms: input.duration_ms,
            timestamp_ms: input.timestamp_ms,
            error: input.error,
            iteration_number: input.iteration_number,
            session_id: input.session_id,
            parent_session_id: input.parent_session_id,
            is_sub_agent: input.is_sub_agent,
        })
    }

    fn save_agent_thinking(
        &self,
        input: MessageAgentThinkingInput,
    ) -> RusqliteResult<MessageAgentThinking> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let metadata_text = input
            .metadata
            .as_ref()
            .map(|value| value.to_string());

        conn.execute(
            "INSERT INTO message_agent_thinking (
                id, message_id, stage, content, timestamp, iteration_number, metadata
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                input.id,
                input.message_id,
                input.stage,
                input.content,
                input.timestamp_ms,
                input.iteration_number,
                metadata_text
            ],
        )?;

        Ok(MessageAgentThinking {
            id: input.id,
            message_id: input.message_id,
            stage: input.stage,
            content: input.content,
            timestamp_ms: input.timestamp_ms,
            iteration_number: input.iteration_number,
            metadata: input.metadata,
        })
    }

    fn get_agent_thinking(
        &self,
        message_id: &str,
    ) -> RusqliteResult<Vec<MessageAgentThinking>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, message_id, stage, content, timestamp, iteration_number, metadata
             FROM message_agent_thinking
             WHERE message_id = ?1
             ORDER BY timestamp ASC",
        )?;

        let rows = stmt.query_map(params![message_id], |row| {
            let metadata_text: Option<String> = row.get(6)?;
            let metadata = metadata_text
                .and_then(|text| serde_json::from_str::<Value>(&text).ok());

            Ok(MessageAgentThinking {
                id: row.get(0)?,
                message_id: row.get(1)?,
                stage: row.get(2)?,
                content: row.get(3)?,
                timestamp_ms: row.get(4)?,
                iteration_number: row.get(5)?,
                metadata,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
    }

    fn get_messages(&self, conversation_id: &str) -> RusqliteResult<Vec<Message>> {
        let start_time = Instant::now();

        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let attachments_dir = attachments_dir()?;

        log::debug!("📁 Setup time: {:?}", start_time.elapsed());
        let messages_query_start = Instant::now();

        let mut messages_stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at ASC",
        )?;

        let mut messages: Vec<Message> = messages_stmt
            .query_map(params![conversation_id], |row| {
                let timestamp: i64 = row.get(4)?;
                Ok(Message {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: Utc.timestamp_opt(timestamp, 0).single().unwrap(),
                    attachments: Vec::new(),
                    tool_executions: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        log::debug!(
            "📨 Messages query time: {:?}",
            messages_query_start.elapsed()
        );
        let attachments_start = Instant::now();

        let mut attachments_stmt = conn.prepare(
            "SELECT message_id, id, name, data, attachment_type, created_at, description, transcript,
             file_path, size_bytes, mime_type, thumbnail_path, updated_at
             FROM message_attachments
             WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)"
        )?;

        let attachments = attachments_stmt.query_map(params![conversation_id], |row| {
            let message_id: String = row.get(0)?;
            let timestamp: i64 = row.get(5)?;
            let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
            let attachment_type: String = row.get(4)?;

            let data = if attachment_type.starts_with("text/") {
                // For text attachments, use the stored content directly
                row.get::<_, String>(3)?
            } else {
                // For binary attachments, read from filesystem and encode
                let file_path: String = row.get(3)?;
                read_attachment_base64_cached(&row.get::<_, String>(1)?, &attachment_type, &file_path, &attachments_dir)?
            };

            // Get updated_at timestamp if available, otherwise use created_at
            let updated_at_timestamp: Option<i64> = row.get(12).ok();
            let updated_at =
                updated_at_timestamp.map(|ts| Utc.timestamp_opt(ts, 0).single().unwrap());

            Ok(MessageAttachment {
                id: Some(row.get(1)?),
                message_id: Some(message_id),
                name: row.get(2)?,
                data,
                attachment_url: None,
                attachment_type,
                description: row.get(6)?,
                transcript: row.get(7)?,
                created_at: Some(created_at),
                updated_at,
                file_path: row.get(8).ok(),
                size_bytes: row.get(9).ok(),
                mime_type: row.get(10).ok(),
                thumbnail_path: row.get(11).ok(),
            })
        })?;

        // Use HashMap for O(1) lookup instead of O(n) iteration
        let mut message_map: HashMap<String, &mut Message> =
            messages.iter_mut().map(|m| (m.id.clone(), m)).collect();

        for attachment in attachments {
            if let Ok(att) = attachment {
                if let Some(message_id) = &att.message_id {
                    if let Some(message) = message_map.get_mut(message_id) {
                        message.attachments.push(att);
                    }
                }
            }
        }

        let tool_executions_start = Instant::now();
        let mut tool_exec_stmt = conn.prepare(
            "SELECT message_id, id, tool_name, parameters, result, success, duration, timestamp, error, iteration_number,
                    session_id, parent_session_id, is_sub_agent
             FROM message_tool_executions
             WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)"
        )?;

        let tool_execs = tool_exec_stmt.query_map(params![conversation_id], |row| {
            let message_id: String = row.get(0)?;
            let timestamp_ms: i64 = row.get(7)?;

            let parameters_raw: String = row.get(3)?;
            let result_raw: String = row.get(4)?;
            let parameters = serde_json::from_str(&parameters_raw)
                .unwrap_or_else(|_| Value::String(parameters_raw));
            let result =
                serde_json::from_str(&result_raw).unwrap_or_else(|_| Value::String(result_raw));

            Ok((
                message_id,
                MessageToolExecution {
                    id: row.get(1)?,
                    message_id: row.get(0)?,
                    tool_name: row.get(2)?,
                    parameters,
                    result,
                    success: row.get(5)?,
                    duration_ms: row.get(6)?,
                    timestamp_ms,
                    error: row.get(8)?,
                    iteration_number: row.get(9)?,
                    session_id: row.get(10)?,
                    parent_session_id: row.get(11)?,
                    is_sub_agent: row.get::<_, Option<i64>>(12)?.unwrap_or(0) != 0,
                },
            ))
        })?;

        for tool_exec in tool_execs {
            if let Ok((message_id, exec)) = tool_exec {
                if let Some(message) = message_map.get_mut(&message_id) {
                    message.tool_executions.push(exec);
                }
            }
        }

        log::debug!(
            "🧰 Tool executions processing time: {:?}",
            tool_executions_start.elapsed()
        );
        log::debug!(
            "📎 Total attachments processing time: {:?}",
            attachments_start.elapsed()
        );
        log::debug!("⏱️  Total get_messages time: {:?}", start_time.elapsed());

        Ok(messages)
    }

    fn get_previous_message_id(
        &self,
        conversation_id: &str,
        exclude_message_id: &str,
    ) -> RusqliteResult<Option<String>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.query_row(
            "SELECT id
             FROM messages
             WHERE conversation_id = ?1 AND id != ?2
             ORDER BY created_at DESC
             LIMIT 1",
            params![conversation_id, exclude_message_id],
            |row| row.get(0),
        )
        .optional()
    }

    fn get_messages_for_prompt(
        &self,
        conversation_id: &str,
        options: PromptHistoryOptions,
    ) -> RusqliteResult<Vec<Message>> {
        let start_time = Instant::now();

        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let messages_query_start = Instant::now();
        let mut messages_stmt = if let Some(_limit) = options.max_messages {
            conn.prepare(
                "SELECT id, conversation_id, role, content, created_at
                 FROM (
                     SELECT id, conversation_id, role, content, created_at
                     FROM messages
                     WHERE conversation_id = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2
                 )
                 ORDER BY created_at ASC",
            )?
        } else {
            conn.prepare(
                "SELECT id, conversation_id, role, content, created_at
                 FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY created_at ASC",
            )?
        };

        let mut messages: Vec<Message> = if let Some(limit) = options.max_messages {
            messages_stmt
                .query_map(params![conversation_id, limit as i64], |row| {
                    let timestamp: i64 = row.get(4)?;
                    Ok(Message {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        role: row.get(2)?,
                        content: row.get(3)?,
                        created_at: Utc.timestamp_opt(timestamp, 0).single().unwrap(),
                        attachments: Vec::new(),
                        tool_executions: Vec::new(),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            messages_stmt
                .query_map(params![conversation_id], |row| {
                    let timestamp: i64 = row.get(4)?;
                    Ok(Message {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        role: row.get(2)?,
                        content: row.get(3)?,
                        created_at: Utc.timestamp_opt(timestamp, 0).single().unwrap(),
                        attachments: Vec::new(),
                        tool_executions: Vec::new(),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };

        log::debug!(
            "📨 Prompt messages query time: {:?}",
            messages_query_start.elapsed()
        );

        messages = trim_messages_by_char_budget(messages, options.max_total_chars);

        if messages.is_empty() {
            log::debug!(
                "⏱️  Prompt get_messages time (empty): {:?}",
                start_time.elapsed()
            );
            return Ok(messages);
        }

        let message_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();

        if options.include_attachments && !message_ids.is_empty() {
            let attachments_start = Instant::now();
            let full_window = options
                .attachments_full_window
                .min(message_ids.len());
            let full_attachment_message_ids: HashSet<String> = messages
                .iter()
                .rev()
                .take(full_window)
                .map(|message| message.id.clone())
                .collect();

            let attachments_dir = if full_window > 0 {
                Some(attachments_dir()?)
            } else {
                None
            };

            let mut message_map: HashMap<String, &mut Message> =
                messages.iter_mut().map(|m| (m.id.clone(), m)).collect();

            let placeholders = build_in_clause(message_ids.len());
            let attachments_sql = format!(
                "SELECT message_id, id, name, data, attachment_type, created_at, description, transcript,
                        file_path, size_bytes, mime_type, thumbnail_path, updated_at
                 FROM message_attachments
                 WHERE message_id IN ({})",
                placeholders
            );
            let mut attachments_stmt = conn.prepare(&attachments_sql)?;

            let attachments = attachments_stmt.query_map(params_from_iter(message_ids.iter()), |row| {
                let message_id: String = row.get(0)?;
                let attachment_id: String = row.get(1)?;
                let name: String = row.get(2)?;
                let data_field: String = row.get(3)?;
                let attachment_type: String = row.get(4)?;
                let timestamp: i64 = row.get(5)?;
                let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
                let description: Option<String> = row.get(6)?;
                let transcript: Option<String> = row.get(7)?;
                let file_path_db: Option<String> = row.get(8).ok();
                let size_bytes: Option<u64> = row.get(9).ok();
                let mime_type: Option<String> = row.get(10).ok();
                let thumbnail_path: Option<String> = row.get(11).ok();
                let updated_at_timestamp: Option<i64> = row.get(12).ok();
                let updated_at = updated_at_timestamp
                    .and_then(|ts| Utc.timestamp_opt(ts, 0).single());

                let should_load_data = full_attachment_message_ids.contains(&message_id);
                let is_text = attachment_type.starts_with("text/")
                    || attachment_type.starts_with("application/json");

                let data = if should_load_data {
                    if is_text {
                        data_field.clone()
                    } else if let Some(dir) = attachments_dir.as_ref() {
                        let file_path = file_path_db
                            .clone()
                            .unwrap_or_else(|| data_field.clone());
                        read_attachment_base64_cached(
                            &attachment_id,
                            &attachment_type,
                            &file_path,
                            dir,
                        )?
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let file_path = file_path_db.or_else(|| {
                    if is_text {
                        None
                    } else {
                        Some(data_field.clone())
                    }
                });

                Ok((
                    message_id,
                    MessageAttachment {
                        id: Some(attachment_id),
                        message_id: None,
                        name,
                        data,
                        attachment_type,
                        description,
                        transcript,
                        created_at: Some(created_at),
                        updated_at,
                        attachment_url: None,
                        file_path,
                        size_bytes,
                        mime_type,
                        thumbnail_path,
                    },
                ))
            })?;

            for attachment in attachments {
                if let Ok((message_id, mut attachment)) = attachment {
                    if let Some(message) = message_map.get_mut(&message_id) {
                        attachment.message_id = Some(message_id.clone());
                        message.attachments.push(attachment);
                    }
                }
            }

            log::debug!(
                "📎 Prompt attachments processing time: {:?}",
                attachments_start.elapsed()
            );
        }

        if options.include_tool_executions && !message_ids.is_empty() {
            let tool_executions_start = Instant::now();
            let placeholders = build_in_clause(message_ids.len());
            let tool_sql = format!(
                "SELECT message_id, id, tool_name, parameters, result, success, duration, timestamp, error, iteration_number,
                        session_id, parent_session_id, is_sub_agent
                 FROM message_tool_executions
                 WHERE message_id IN ({})",
                placeholders
            );
            let mut tool_exec_stmt = conn.prepare(&tool_sql)?;
            let tool_execs =
                tool_exec_stmt.query_map(params_from_iter(message_ids.iter()), |row| {
                    let message_id: String = row.get(0)?;
                    let timestamp_ms: i64 = row.get(7)?;

                    let parameters_raw: String = row.get(3)?;
                    let result_raw: String = row.get(4)?;
                    let parameters = serde_json::from_str(&parameters_raw)
                        .unwrap_or_else(|_| Value::String(parameters_raw));
                    let result =
                        serde_json::from_str(&result_raw).unwrap_or_else(|_| Value::String(result_raw));

                    Ok((
                        message_id,
                        MessageToolExecution {
                            id: row.get(1)?,
                            message_id: row.get(0)?,
                            tool_name: row.get(2)?,
                            parameters,
                            result,
                            success: row.get(5)?,
                            duration_ms: row.get(6)?,
                            timestamp_ms,
                            error: row.get(8)?,
                            iteration_number: row.get(9)?,
                            session_id: row.get(10)?,
                            parent_session_id: row.get(11)?,
                            is_sub_agent: row.get::<_, Option<i64>>(12)?.unwrap_or(0) != 0,
                        },
                    ))
                })?;

            let mut message_map: HashMap<String, &mut Message> =
                messages.iter_mut().map(|m| (m.id.clone(), m)).collect();

            for tool_exec in tool_execs {
                if let Ok((message_id, exec)) = tool_exec {
                    if let Some(message) = message_map.get_mut(&message_id) {
                        message.tool_executions.push(exec);
                    }
                }
            }

            log::debug!(
                "🧰 Prompt tool executions processing time: {:?}",
                tool_executions_start.elapsed()
            );
        }

        log::debug!(
            "⏱️  Total get_messages_for_prompt time: {:?}",
            start_time.elapsed()
        );

        Ok(messages)
    }

    fn save_attachment_to_fs(&self, data: &str, file_name: &str) -> RusqliteResult<String> {
        let app_dir = path::app_data_dir(&tauri::Config::default()).ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("Failed to get app directory".into())
        })?;

        let attachments_dir = app_dir.join("dev.michalmlak.ai_agent").join("attachments");
        fs::create_dir_all(&attachments_dir)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        let unique_filename = format!("{}-{}", Uuid::new_v4(), file_name);
        let file_path = attachments_dir.join(&unique_filename);

        let base64_data = if data.starts_with("data:") {
            data.split(",").nth(1).ok_or_else(|| {
                rusqlite::Error::InvalidParameterName("Invalid data URL format".into())
            })?
        } else {
            data
        };

        let decoded_data = base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        fs::write(&file_path, &decoded_data)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        Ok(unique_filename)
    }
}
