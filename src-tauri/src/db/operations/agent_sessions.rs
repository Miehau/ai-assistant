use chrono::{TimeZone, Utc};
use rusqlite::{params, Result as RusqliteResult};
use serde_json::Value;

use crate::db::models::{
    AgentConfig,
    AgentSession,
    ApprovalDecision,
    PhaseKind,
    Plan,
    PlanStep,
    StepAction,
    StepApproval,
    StepResult,
    StepStatus,
};
use super::DbOperations;

pub trait AgentSessionOperations: DbOperations {
    fn save_agent_session(&self, session: &AgentSession) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "INSERT INTO agent_sessions (
                id, conversation_id, message_id, phase, phase_data, config,
                created_at, updated_at, completed_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session.id,
                session.conversation_id,
                session.message_id,
                session.phase.kind(),
                serde_json::to_string(&session.phase).unwrap_or_else(|_| "{}".to_string()),
                serde_json::to_string(&session.config).unwrap_or_else(|_| "{}".to_string()),
                session.created_at.timestamp(),
                session.updated_at.timestamp(),
                session.completed_at.map(|v| v.timestamp()),
            ],
        )?;

        Ok(())
    }

    fn update_agent_session_phase(&self, session_id: &str, phase: &PhaseKind) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "UPDATE agent_sessions
             SET phase = ?1, phase_data = ?2, updated_at = ?3
             WHERE id = ?4",
            params![
                phase.kind(),
                serde_json::to_string(phase).unwrap_or_else(|_| "{}".to_string()),
                Utc::now().timestamp(),
                session_id,
            ],
        )?;

        Ok(())
    }

    fn update_agent_session_completed(&self, session_id: &str, final_response: &str) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let phase = PhaseKind::Complete {
            final_response: final_response.to_string(),
        };

        conn.execute(
            "UPDATE agent_sessions
             SET phase = ?1, phase_data = ?2, updated_at = ?3, completed_at = ?4
             WHERE id = ?5",
            params![
                phase.kind(),
                serde_json::to_string(&phase).unwrap_or_else(|_| "{}".to_string()),
                Utc::now().timestamp(),
                Utc::now().timestamp(),
                session_id,
            ],
        )?;

        Ok(())
    }

    fn save_agent_plan(&self, session_id: &str, plan: &Plan) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "INSERT INTO agent_plans (
                id, session_id, goal, assumptions, revision_number, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                plan.id,
                session_id,
                plan.goal,
                serde_json::to_string(&plan.assumptions).unwrap_or_else(|_| "[]".to_string()),
                plan.revision_count as i64,
                plan.created_at.timestamp(),
            ],
        )?;

        Ok(())
    }

    fn save_plan_steps(&self, plan_id: &str, steps: &[PlanStep]) -> RusqliteResult<()> {
        let binding = self.conn();
        let mut conn = binding.lock().unwrap();
        let tx = conn.transaction()?;

        for step in steps {
            let (action_type, action_data) = serialize_step_action(&step.action);
            tx.execute(
                "INSERT INTO agent_plan_steps (
                    id, plan_id, sequence, description, expected_outcome,
                    action_type, action_data, status, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    step.id,
                    plan_id,
                    step.sequence as i64,
                    step.description,
                    step.expected_outcome,
                    action_type,
                    action_data,
                    step_status_to_str(&step.status),
                    Utc::now().timestamp(),
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn update_plan_step_status(&self, step_id: &str, status: StepStatus) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "UPDATE agent_plan_steps SET status = ?1 WHERE id = ?2",
            params![step_status_to_str(&status), step_id],
        )?;
        Ok(())
    }

    fn save_step_result(&self, result: &StepResult) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        conn.execute(
            "INSERT INTO agent_step_results (
                id, step_id, session_id, success, output, error, duration_ms, completed_at
            )
            VALUES (?1, ?2, (SELECT session_id FROM agent_plans WHERE id = (SELECT plan_id FROM agent_plan_steps WHERE id = ?2)), ?3, ?4, ?5, ?6, ?7)",
            params![
                result.step_id.clone(),
                result.step_id,
                result.success as i32,
                result.output.as_ref().map(|v| v.to_string()),
                result.error,
                result.duration_ms,
                result.completed_at.timestamp(),
            ],
        )?;

        Ok(())
    }

    fn save_step_approval(&self, step_id: &str, approval: &StepApproval) -> RusqliteResult<()> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let (decision, auto_reason) = match &approval.decision {
            ApprovalDecision::Approved => ("approved", None),
            ApprovalDecision::Skipped => ("skipped", None),
            ApprovalDecision::Modified => ("modified", None),
            ApprovalDecision::Denied => ("denied", None),
            ApprovalDecision::AutoApproved { reason } => ("auto_approved", Some(reason.clone())),
        };

        conn.execute(
            "INSERT INTO agent_step_approvals (
                id, step_id, decision, auto_approve_reason, feedback, decided_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                step_id,
                step_id,
                decision,
                auto_reason,
                approval.feedback,
                approval.decided_at.timestamp(),
            ],
        )?;

        Ok(())
    }

    #[allow(dead_code)]
    fn find_incomplete_session(&self, conversation_id: &str) -> RusqliteResult<Option<AgentSession>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, message_id, phase_data, config, created_at, updated_at, completed_at
             FROM agent_sessions
             WHERE conversation_id = ?1 AND completed_at IS NULL
             ORDER BY updated_at DESC
             LIMIT 1",
        )?;

        let mut rows = stmt.query(params![conversation_id])?;
        if let Some(row) = rows.next()? {
            let phase_data: String = row.get(3)?;
            let config_data: String = row.get(4)?;
            let created_at: i64 = row.get(5)?;
            let updated_at: i64 = row.get(6)?;
            let completed_at: Option<i64> = row.get(7)?;

            let phase: PhaseKind = serde_json::from_str(&phase_data).unwrap_or(PhaseKind::Triage);
            let config: AgentConfig =
                serde_json::from_str(&config_data).unwrap_or_else(|_| AgentConfig::default());

            let plan = self.load_latest_plan(row.get::<_, String>(0)?.as_str()).ok().flatten();
            let gathered_info = Vec::new();
            let step_results = Vec::new();

            return Ok(Some(AgentSession {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                message_id: row.get(2)?,
                phase,
                plan,
                gathered_info,
                step_results,
                config,
                created_at: Utc.timestamp_opt(created_at, 0).single().unwrap(),
                updated_at: Utc.timestamp_opt(updated_at, 0).single().unwrap(),
                completed_at: completed_at
                    .and_then(|ts| Utc.timestamp_opt(ts, 0).single()),
            }));
        }

        Ok(None)
    }

    #[allow(dead_code)]
    fn load_latest_plan(&self, session_id: &str) -> RusqliteResult<Option<Plan>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, goal, assumptions, revision_number, created_at
             FROM agent_plans
             WHERE session_id = ?1
             ORDER BY revision_number DESC, created_at DESC
             LIMIT 1",
        )?;

        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            let plan_id: String = row.get(0)?;
            let assumptions_data: String = row.get(2)?;
            let created_at: i64 = row.get(4)?;
            let assumptions: Vec<String> =
                serde_json::from_str(&assumptions_data).unwrap_or_default();
            let steps = self.load_plan_steps(&plan_id)?;

            return Ok(Some(Plan {
                id: plan_id,
                goal: row.get(1)?,
                assumptions,
                steps,
                revision_count: row.get::<_, i64>(3)? as u32,
                created_at: Utc.timestamp_opt(created_at, 0).single().unwrap(),
            }));
        }

        Ok(None)
    }

    #[allow(dead_code)]
    fn load_plan_steps(&self, plan_id: &str) -> RusqliteResult<Vec<PlanStep>> {
        let binding = self.conn();
        let conn = binding.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, sequence, description, expected_outcome, action_type, action_data, status
             FROM agent_plan_steps
             WHERE plan_id = ?1
             ORDER BY sequence ASC",
        )?;

        let steps = stmt
            .query_map(params![plan_id], |row| {
                let action_type: String = row.get(4)?;
                let action_data: String = row.get(5)?;
                let status: String = row.get(6)?;
                Ok(PlanStep {
                    id: row.get(0)?,
                    sequence: row.get::<_, i64>(1)? as usize,
                    description: row.get(2)?,
                    expected_outcome: row.get(3)?,
                    action: parse_step_action(&action_type, &action_data),
                    status: step_status_from_str(&status),
                    result: None,
                    approval: None,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(steps)
    }
}

fn serialize_step_action(action: &StepAction) -> (String, String) {
    match action {
        StepAction::ToolCall { tool, args } => (
            "tool_call".to_string(),
            serde_json::to_string(&serde_json::json!({ "tool": tool, "args": args }))
                .unwrap_or_else(|_| "{}".to_string()),
        ),
        StepAction::AskUser { question } => (
            "ask_user".to_string(),
            serde_json::to_string(&serde_json::json!({ "question": question }))
                .unwrap_or_else(|_| "{}".to_string()),
        ),
        StepAction::Think { prompt } => (
            "think".to_string(),
            serde_json::to_string(&serde_json::json!({ "prompt": prompt }))
                .unwrap_or_else(|_| "{}".to_string()),
        ),
    }
}

#[allow(dead_code)]
fn parse_step_action(action_type: &str, action_data: &str) -> StepAction {
    let data: Value = serde_json::from_str(action_data).unwrap_or_else(|_| Value::Null);
    match action_type {
        "tool_call" => StepAction::ToolCall {
            tool: data.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            args: data.get("args").cloned().unwrap_or_else(|| Value::Object(Default::default())),
        },
        "ask_user" => StepAction::AskUser {
            question: data
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        "think" => StepAction::Think {
            prompt: data
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        _ => StepAction::Think {
            prompt: "".to_string(),
        },
    }
}

fn step_status_to_str(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Pending => "pending",
        StepStatus::Proposed => "proposed",
        StepStatus::Approved => "approved",
        StepStatus::Executing => "executing",
        StepStatus::Completed => "completed",
        StepStatus::Failed => "failed",
        StepStatus::Skipped => "skipped",
    }
}

#[allow(dead_code)]
fn step_status_from_str(value: &str) -> StepStatus {
    match value {
        "proposed" => StepStatus::Proposed,
        "approved" => StepStatus::Approved,
        "executing" => StepStatus::Executing,
        "completed" => StepStatus::Completed,
        "failed" => StepStatus::Failed,
        "skipped" => StepStatus::Skipped,
        _ => StepStatus::Pending,
    }
}
