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
pub enum ResumeTarget {
    Clarifying,
    Planning { revision: u32 },
    ProposingStep { step_index: usize },
    Executing { step_id: String, tool_iteration: u32 },
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

    pub fn is_valid_transition(&self, next: &PhaseKind) -> bool {
        match (self, next) {
            (PhaseKind::Controller, PhaseKind::Controller) => true,
            (PhaseKind::Controller, PhaseKind::Executing { .. }) => true,
            (PhaseKind::Controller, PhaseKind::Complete { .. }) => true,
            (PhaseKind::Controller, PhaseKind::GuardrailStop { .. }) => true,
            (PhaseKind::Controller, PhaseKind::NeedsHumanInput { .. }) => true,
            (PhaseKind::Executing { .. }, PhaseKind::Controller) => true,

            (PhaseKind::Triage, PhaseKind::Complete { .. }) => true,
            (PhaseKind::Triage, PhaseKind::Clarifying { .. }) => true,
            (PhaseKind::Triage, PhaseKind::Planning { revision: 0 }) => true,

            (PhaseKind::Clarifying { .. }, PhaseKind::Planning { .. }) => true,
            (PhaseKind::Clarifying { .. }, PhaseKind::NeedsHumanInput { .. }) => true,
            (PhaseKind::Clarifying { .. }, PhaseKind::GuardrailStop { .. }) => true,

            (PhaseKind::Planning { .. }, PhaseKind::ProposingStep { .. }) => true,
            (PhaseKind::Planning { .. }, PhaseKind::NeedsHumanInput { .. }) => true,
            (PhaseKind::Planning { .. }, PhaseKind::GuardrailStop { .. }) => true,

            (PhaseKind::ProposingStep { .. }, PhaseKind::Executing { .. }) => true,
            (PhaseKind::ProposingStep { .. }, PhaseKind::ProposingStep { .. }) => true,
            (PhaseKind::ProposingStep { .. }, PhaseKind::Planning { .. }) => true,
            (PhaseKind::ProposingStep { .. }, PhaseKind::NeedsHumanInput { .. }) => true,
            (PhaseKind::ProposingStep { .. }, PhaseKind::Complete { .. }) => true,
            (PhaseKind::ProposingStep { .. }, PhaseKind::GuardrailStop { .. }) => true,

            (PhaseKind::Executing { .. }, PhaseKind::Reflecting) => true,
            (PhaseKind::Executing { .. }, PhaseKind::NeedsHumanInput { .. }) => true,
            (PhaseKind::Executing { .. }, PhaseKind::GuardrailStop { .. }) => true,

            (PhaseKind::Reflecting, PhaseKind::ProposingStep { .. }) => true,
            (PhaseKind::Reflecting, PhaseKind::Planning { .. }) => true,
            (PhaseKind::Reflecting, PhaseKind::Clarifying { .. }) => true,
            (PhaseKind::Reflecting, PhaseKind::Complete { .. }) => true,
            (PhaseKind::Reflecting, PhaseKind::NeedsHumanInput { .. }) => true,

            (PhaseKind::NeedsHumanInput { resume_to, .. }, next) => match (resume_to, next) {
                (ResumeTarget::Clarifying, PhaseKind::Clarifying { .. }) => true,
                (ResumeTarget::Planning { .. }, PhaseKind::Planning { .. }) => true,
                (ResumeTarget::ProposingStep { .. }, PhaseKind::ProposingStep { .. }) => true,
                (ResumeTarget::Executing { .. }, PhaseKind::Executing { .. }) => true,
                (ResumeTarget::Reflecting, PhaseKind::Reflecting) => true,
                _ => false,
            },

            _ => false,
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
    ToolCall { tool: String, args: serde_json::Value },
    AskUser { question: String },
    Think { prompt: String },
    Respond { message: String },
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
