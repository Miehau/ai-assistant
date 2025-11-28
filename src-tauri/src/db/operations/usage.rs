use rusqlite::{params, Result as RusqliteResult};
use chrono::{TimeZone, Utc, DateTime};
use crate::db::models::{MessageUsage, ConversationUsageSummary, SaveMessageUsageInput, UsageStatistics, ModelUsage, DailyUsage, DailyModelUsage};
use super::DbOperations;
use uuid::Uuid;

pub trait UsageOperations: DbOperations {
    /// Save token usage data for a message
    fn save_message_usage(&self, input: SaveMessageUsageInput) -> RusqliteResult<MessageUsage> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        let created_at_timestamp = created_at.timestamp();
        let total_tokens = input.prompt_tokens + input.completion_tokens;

        conn.execute(
            "INSERT INTO message_usage (id, message_id, model_name, prompt_tokens, completion_tokens, total_tokens, estimated_cost, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                input.message_id,
                input.model_name,
                input.prompt_tokens,
                input.completion_tokens,
                total_tokens,
                input.estimated_cost,
                created_at_timestamp
            ],
        )?;

        Ok(MessageUsage {
            id,
            message_id: input.message_id,
            model_name: input.model_name,
            prompt_tokens: input.prompt_tokens,
            completion_tokens: input.completion_tokens,
            total_tokens,
            estimated_cost: input.estimated_cost,
            created_at,
        })
    }

    /// Update or create conversation usage summary
    fn update_conversation_usage(&self, conversation_id: &str) -> RusqliteResult<ConversationUsageSummary> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        // Get message IDs for this conversation
        let mut stmt = conn.prepare(
            "SELECT id FROM messages WHERE conversation_id = ?1"
        )?;
        let message_ids: Vec<String> = stmt.query_map(params![conversation_id], |row| {
            row.get(0)
        })?.collect::<Result<_, _>>()?;

        // Calculate totals from message_usage
        let mut total_prompt_tokens = 0;
        let mut total_completion_tokens = 0;
        let mut total_cost = 0.0;
        let message_count = message_ids.len() as i32;

        for message_id in message_ids {
            if let Ok(usage) = conn.query_row(
                "SELECT prompt_tokens, completion_tokens, estimated_cost FROM message_usage WHERE message_id = ?1",
                params![message_id],
                |row| {
                    Ok((
                        row.get::<_, i32>(0)?,
                        row.get::<_, i32>(1)?,
                        row.get::<_, f64>(2)?
                    ))
                }
            ) {
                total_prompt_tokens += usage.0;
                total_completion_tokens += usage.1;
                total_cost += usage.2;
            }
        }

        let total_tokens = total_prompt_tokens + total_completion_tokens;
        let last_updated = Utc::now();
        let last_updated_timestamp = last_updated.timestamp();

        // Upsert conversation usage summary
        conn.execute(
            "INSERT INTO conversation_usage_summary
             (conversation_id, total_prompt_tokens, total_completion_tokens, total_tokens, total_cost, message_count, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(conversation_id) DO UPDATE SET
                total_prompt_tokens = ?2,
                total_completion_tokens = ?3,
                total_tokens = ?4,
                total_cost = ?5,
                message_count = ?6,
                last_updated = ?7",
            params![
                conversation_id,
                total_prompt_tokens,
                total_completion_tokens,
                total_tokens,
                total_cost,
                message_count,
                last_updated_timestamp
            ],
        )?;

        Ok(ConversationUsageSummary {
            conversation_id: conversation_id.to_string(),
            total_prompt_tokens,
            total_completion_tokens,
            total_tokens,
            total_cost,
            message_count,
            last_updated,
        })
    }

    /// Get usage summary for a specific conversation
    fn get_conversation_usage(&self, conversation_id: &str) -> RusqliteResult<Option<ConversationUsageSummary>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let result = conn.query_row(
            "SELECT conversation_id, total_prompt_tokens, total_completion_tokens, total_tokens, total_cost, message_count, last_updated
             FROM conversation_usage_summary WHERE conversation_id = ?1",
            params![conversation_id],
            |row| {
                let timestamp: i64 = row.get(6)?;
                let last_updated = Utc.timestamp_opt(timestamp, 0).single().unwrap();
                Ok(ConversationUsageSummary {
                    conversation_id: row.get(0)?,
                    total_prompt_tokens: row.get(1)?,
                    total_completion_tokens: row.get(2)?,
                    total_tokens: row.get(3)?,
                    total_cost: row.get(4)?,
                    message_count: row.get(5)?,
                    last_updated,
                })
            }
        );

        match result {
            Ok(summary) => Ok(Some(summary)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get usage statistics for a date range
    fn get_usage_statistics(&self, start_date: Option<DateTime<Utc>>, end_date: Option<DateTime<Utc>>) -> RusqliteResult<UsageStatistics> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let start_timestamp = start_date.map(|d| d.timestamp()).unwrap_or(0);
        let end_timestamp = end_date.map(|d| d.timestamp()).unwrap_or(i64::MAX);

        // Get total statistics
        let (total_messages, total_tokens, total_cost): (i32, i32, f64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0), COALESCE(SUM(estimated_cost), 0.0)
             FROM message_usage
             WHERE created_at >= ?1 AND created_at <= ?2",
            params![start_timestamp, end_timestamp],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        )?;

        // Get usage by model
        let mut stmt = conn.prepare(
            "SELECT model_name, COUNT(*), SUM(total_tokens), SUM(estimated_cost)
             FROM message_usage
             WHERE created_at >= ?1 AND created_at <= ?2
             GROUP BY model_name
             ORDER BY SUM(estimated_cost) DESC"
        )?;
        let by_model = stmt.query_map(params![start_timestamp, end_timestamp], |row| {
            Ok(ModelUsage {
                model_name: row.get(0)?,
                message_count: row.get(1)?,
                total_tokens: row.get(2)?,
                total_cost: row.get(3)?,
            })
        })?.collect::<Result<_, _>>()?;

        // Get usage by date
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at, 'unixepoch') as date, COUNT(*), SUM(total_tokens), SUM(estimated_cost)
             FROM message_usage
             WHERE created_at >= ?1 AND created_at <= ?2
             GROUP BY date
             ORDER BY date DESC"
        )?;
        let by_date = stmt.query_map(params![start_timestamp, end_timestamp], |row| {
            Ok(DailyUsage {
                date: row.get(0)?,
                message_count: row.get(1)?,
                total_tokens: row.get(2)?,
                total_cost: row.get(3)?,
            })
        })?.collect::<Result<_, _>>()?;

        // Get usage by model and date (for stacked bar chart)
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at, 'unixepoch') as date, model_name, COUNT(*), SUM(total_tokens), SUM(estimated_cost)
             FROM message_usage
             WHERE created_at >= ?1 AND created_at <= ?2
             GROUP BY date, model_name
             ORDER BY date DESC, model_name"
        )?;
        let by_model_date = stmt.query_map(params![start_timestamp, end_timestamp], |row| {
            Ok(DailyModelUsage {
                date: row.get(0)?,
                model_name: row.get(1)?,
                message_count: row.get(2)?,
                total_tokens: row.get(3)?,
                total_cost: row.get(4)?,
            })
        })?.collect::<Result<_, _>>()?;

        Ok(UsageStatistics {
            total_messages,
            total_tokens,
            total_cost,
            by_model,
            by_date,
            by_model_date,
        })
    }

    /// Get usage for a specific message
    fn get_message_usage(&self, message_id: &str) -> RusqliteResult<Option<MessageUsage>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let result = conn.query_row(
            "SELECT id, message_id, model_name, prompt_tokens, completion_tokens, total_tokens, estimated_cost, created_at
             FROM message_usage WHERE message_id = ?1",
            params![message_id],
            |row| {
                let timestamp: i64 = row.get(7)?;
                let created_at = Utc.timestamp_opt(timestamp, 0).single().unwrap();
                Ok(MessageUsage {
                    id: row.get(0)?,
                    message_id: row.get(1)?,
                    model_name: row.get(2)?,
                    prompt_tokens: row.get(3)?,
                    completion_tokens: row.get(4)?,
                    total_tokens: row.get(5)?,
                    estimated_cost: row.get(6)?,
                    created_at,
                })
            }
        );

        match result {
            Ok(usage) => Ok(Some(usage)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
