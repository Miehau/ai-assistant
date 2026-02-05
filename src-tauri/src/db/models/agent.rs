use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub phase: PhaseKind,
    pub plan: Option<Plan>,
    pub gathered_info: Vec<GatheredInfo>,
    pub step_results: Vec<StepResult>,
    pub config: AgentConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PhaseKind {
    Controller,
    Triage,
    Clarifying {
        attempts: u32,
        pending_questions: Vec<String>,
    },
    Planning {
        revision: u32,
    },
    ProposingStep {
        step_index: usize,
    },
    Executing {
        step_id: String,
        tool_iteration: u32,
    },
    Reflecting,
    Complete {
        final_response: String,
    },
    NeedsHumanInput {
        question: String,
        context: Option<String>,
        resume_to: ResumeTarget,
    },
    GuardrailStop {
        reason: String,
        recoverable: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeTarget {
    #[serde(alias = "Controller")]
    Controller,
    #[serde(alias = "Clarifying")]
    Clarifying,
    #[serde(alias = "Planning")]
    Planning { revision: u32 },
    #[serde(alias = "ProposingStep")]
    ProposingStep { step_index: usize },
    #[serde(alias = "Executing")]
    Executing {
        step_id: String,
        tool_iteration: u32,
    },
    #[serde(alias = "Reflecting")]
    Reflecting,
}

impl PhaseKind {
    pub fn kind(&self) -> &'static str {
        match self {
            PhaseKind::Controller => "controller",
            PhaseKind::Triage => "triage",
            PhaseKind::Clarifying { .. } => "clarifying",
            PhaseKind::Planning { .. } => "planning",
            PhaseKind::ProposingStep { .. } => "proposing_step",
            PhaseKind::Executing { .. } => "executing",
            PhaseKind::Reflecting => "reflecting",
            PhaseKind::Complete { .. } => "complete",
            PhaseKind::NeedsHumanInput { .. } => "needs_human_input",
            PhaseKind::GuardrailStop { .. } => "guardrail_stop",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal: String,
    pub assumptions: Vec<String>,
    pub steps: Vec<PlanStep>,
    pub revision_count: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub sequence: usize,
    pub description: String,
    pub expected_outcome: String,
    pub action: StepAction,
    pub status: StepStatus,
    pub result: Option<StepResult>,
    pub approval: Option<StepApproval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepAction {
    ToolCall {
        tool: String,
        args: serde_json::Value,
    },
    AskUser {
        question: String,
    },
    Think {
        prompt: String,
    },
    Respond {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Pending,
    Proposed,
    Approved,
    Executing,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub tool_executions: Vec<ToolExecutionRecord>,
    pub duration_ms: i64,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepApproval {
    pub decision: ApprovalDecision,
    pub feedback: Option<String>,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApprovalDecision {
    Approved,
    Skipped,
    Modified,
    Denied,
    AutoApproved { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatheredInfo {
    pub question: String,
    pub answer: String,
    pub source: InfoSource,
    pub gathered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InfoSource {
    Tool { tool_name: String },
    User,
    Assumption,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub max_total_llm_turns: u32,
    pub max_clarify_iters: u32,
    pub max_plan_revisions: u32,
    pub max_tool_calls_per_step: u32,
    pub approval_timeout_ms: u64,
    pub tool_execution_timeout_ms: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_total_llm_turns: 20,
            max_clarify_iters: 3,
            max_plan_revisions: 3,
            max_tool_calls_per_step: 5,
            approval_timeout_ms: 60_000,
            tool_execution_timeout_ms: 120_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub execution_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub success: bool,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub iteration: usize,
    pub timestamp_ms: i64,
}
