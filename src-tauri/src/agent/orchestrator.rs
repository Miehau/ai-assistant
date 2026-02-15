use crate::agent::prompts::CONTROLLER_PROMPT_BASE;
use crate::db::{
    AgentConfig, AgentSession, AgentSessionOperations, MessageToolExecutionInput, PhaseKind, Plan,
    PlanStep, ResumeTarget, StepAction, StepResult, StepStatus, ToolBatchToolCall,
    ToolExecutionRecord,
};
#[cfg(debug_assertions)]
use crate::db::{MessageAgentThinkingInput, MessageOperations};
use crate::events::{
    AgentEvent, EventBus, EVENT_AGENT_COMPLETED, EVENT_AGENT_PHASE_CHANGED,
    EVENT_AGENT_PLAN_ADJUSTED, EVENT_AGENT_PLAN_CREATED, EVENT_AGENT_STEP_COMPLETED,
    EVENT_AGENT_STEP_PROPOSED, EVENT_AGENT_STEP_STARTED, EVENT_TOOL_EXECUTION_APPROVED,
    EVENT_TOOL_EXECUTION_COMPLETED, EVENT_TOOL_EXECUTION_DENIED, EVENT_TOOL_EXECUTION_PROPOSED,
    EVENT_TOOL_EXECUTION_STARTED,
};
use crate::llm::{json_schema_output_format, LlmMessage, StreamResult};
use crate::tool_outputs::{store_tool_output, tool_output_exists, ToolOutputRecord};
use crate::tools::{
    get_conversation_tool_approval_override, get_tool_approval_override,
    load_conversation_tool_approval_overrides, load_tool_approval_overrides, ApprovalStore,
    PendingToolApprovalInput, ToolApprovalDecision, ToolDefinition, ToolExecutionContext,
    ToolRegistry, ToolResultMode,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use uuid::Uuid;

const AUTO_INLINE_RESULT_MAX_CHARS: usize = 4_096;
const INLINE_RESULT_HARD_MAX_CHARS: usize = 16_384;
const PERSISTED_RESULT_PREVIEW_MAX_CHARS: usize = 1_200;
const CONTROLLER_HISTORY_MAX_CHARS: usize = 48_000;
const CONTROLLER_HISTORY_STABLE_PREFIX_MESSAGES: usize = 8;
const CONTROLLER_HISTORY_RECENT_TAIL_MESSAGES: usize = 20;
const CONTROLLER_JSON_START_MARKER: &str = "=====JSON_START=====";
const CONTROLLER_JSON_END_MARKER: &str = "=====JSON_END=====";
const PARALLEL_BATCH_FALLBACK_TIMEOUT_MS: u64 = 120_000;
const CONTROLLER_TOOL_SUMMARY_MAX_CHARS: usize = 2_000;
const CONTROLLER_TOOL_SUMMARY_MAX_ARGS_CHARS: usize = 400;
const CONTROLLER_TOOL_SUMMARY_MAX_RESULT_CHARS: usize = 800;
const CONTROLLER_TOOL_SUMMARY_MAX_METADATA_CHARS: usize = 320;
const OUTPUT_METADATA_MAX_TOP_LEVEL_KEYS: usize = 20;
const OUTPUT_METADATA_MAX_ID_HINTS: usize = 12;
const OUTPUT_METADATA_MAX_ID_SAMPLE_CHARS: usize = 80;
const OUTPUT_METADATA_MAX_ITEM_TYPE_HINTS: usize = 8;
const OUTPUT_METADATA_SCAN_MAX_DEPTH: usize = 4;
const OUTPUT_METADATA_SCAN_MAX_ARRAY_ITEMS: usize = 24;
const OUTPUT_METADATA_MAX_SERIALIZED_CHARS: usize = 1_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputModeHint {
    Auto,
    Inline,
    Persist,
}

impl OutputModeHint {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Inline => "inline",
            Self::Persist => "persist",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "inline" => Some(Self::Inline),
            "persist" => Some(Self::Persist),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolvedOutputMode {
    Inline,
    Persist,
}

impl ResolvedOutputMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Persist => "persist",
        }
    }
}

#[derive(Clone, Debug)]
struct OutputDeliveryResolution {
    requested_output_mode: OutputModeHint,
    resolved_output_mode: ResolvedOutputMode,
    forced_persist: bool,
    forced_reason: Option<&'static str>,
}

pub struct DynamicController {
    db: crate::db::Db,
    event_bus: EventBus,
    tool_registry: ToolRegistry,
    approvals: ApprovalStore,
    cancel_flag: Arc<AtomicBool>,
    session: AgentSession,
    messages: Vec<LlmMessage>,
    assistant_message_id: String,
    pending_tool_executions: Vec<MessageToolExecutionInput>,
    last_step_result: Option<StepResult>,
    tool_calls_in_current_step: u32,
    requested_user_input: bool,
}

impl DynamicController {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: crate::db::Db,
        event_bus: EventBus,
        tool_registry: ToolRegistry,
        approvals: ApprovalStore,
        cancel_flag: Arc<AtomicBool>,
        messages: Vec<LlmMessage>,
        conversation_id: String,
        message_id: String,
        assistant_message_id: String,
    ) -> Result<Self, String> {
        let now = Utc::now();
        let session = AgentSession {
            id: Uuid::new_v4().to_string(),
            conversation_id,
            message_id,
            phase: PhaseKind::Controller,
            plan: None,
            gathered_info: Vec::new(),
            step_results: Vec::new(),
            config: AgentConfig::default(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };

        AgentSessionOperations::save_agent_session(&db, &session).map_err(|e| e.to_string())?;

        Ok(Self {
            db,
            event_bus,
            tool_registry,
            approvals,
            cancel_flag,
            session,
            messages,
            assistant_message_id,
            pending_tool_executions: Vec::new(),
            last_step_result: None,
            tool_calls_in_current_step: 0,
            requested_user_input: false,
        })
    }

    pub fn run<F>(&mut self, user_message: &str, call_llm: &mut F) -> Result<String, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        self.set_phase(PhaseKind::Controller)?;

        let mut turns = 0u32;
        loop {
            if self.is_cancelled() {
                return Err("Cancelled".to_string());
            }
            if turns >= self.session.config.max_total_llm_turns {
                return Err("Exceeded maximum LLM turns".to_string());
            }
            turns += 1;
            self.tool_calls_in_current_step = 0;

            let decision = self.call_controller(call_llm)?;
            match decision {
                ControllerAction::NextStep {
                    thinking: _thinking,
                    step_type,
                    description,
                    tool,
                    tools,
                    args,
                    output_mode,
                    message,
                    question,
                    context,
                    resume_to,
                } => {
                    self.ensure_plan(user_message)?;
                    let effective_type = step_type
                        .as_deref()
                        .or_else(|| infer_step_type_flat(&tool, &tools, &message, &question))
                        .unwrap_or("tool"); // safe: validate() already checked
                    match self.execute_flat_step(
                        call_llm,
                        effective_type,
                        description,
                        tool,
                        tools,
                        args,
                        output_mode,
                        message,
                        question,
                        context,
                        resume_to,
                    )? {
                        StepExecutionOutcome::Continue => {}
                        StepExecutionOutcome::Complete(response) => {
                            return self.finish(response);
                        }
                    }
                }
                ControllerAction::Complete { message } => {
                    return self.finish(message);
                }
                ControllerAction::GuardrailStop { reason, message } => {
                    let detail = message.unwrap_or_else(|| reason.clone());
                    self.set_phase(PhaseKind::GuardrailStop {
                        reason,
                        recoverable: false,
                    })?;
                    return Err(detail);
                }
                ControllerAction::AskUser {
                    question,
                    context,
                    resume_to,
                } => {
                    self.requested_user_input = true;
                    let _ = (&context, &resume_to);
                    return self.finish(question);
                }
            }
        }
    }

    #[cfg(debug_assertions)]
    fn record_trace(
        &self,
        stage: &str,
        content: String,
        iteration: i64,
        metadata: Option<Value>,
    ) {
        let _ = MessageOperations::save_agent_thinking(
            &self.db,
            MessageAgentThinkingInput {
                id: Uuid::new_v4().to_string(),
                message_id: self.assistant_message_id.clone(),
                stage: stage.to_string(),
                content,
                timestamp_ms: Utc::now().timestamp_millis(),
                iteration_number: iteration,
                metadata,
            },
        );
    }

    #[cfg(not(debug_assertions))]
    fn record_trace(
        &self,
        _stage: &str,
        _content: String,
        _iteration: i64,
        _metadata: Option<Value>,
    ) {
    }

    fn finish(&mut self, response: String) -> Result<String, String> {
        AgentSessionOperations::update_agent_session_completed(
            &self.db,
            &self.session.id,
            &response,
        )
        .map_err(|e| e.to_string())?;
        let now = Utc::now();
        self.session.phase = PhaseKind::Complete {
            final_response: response.clone(),
        };
        self.session.updated_at = now;
        self.session.completed_at = Some(now);
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_COMPLETED,
            json!({
                "session_id": self.session.id,
                "response": response.clone(),
            }),
            Utc::now().timestamp_millis(),
        ));
        Ok(response)
    }

    fn ensure_plan(&mut self, user_message: &str) -> Result<(), String> {
        if self.session.plan.is_some() {
            return Ok(());
        }

        let now = Utc::now();
        let goal = summarize_goal(user_message);
        let plan = Plan {
            id: Uuid::new_v4().to_string(),
            goal,
            assumptions: Vec::new(),
            steps: Vec::new(),
            revision_count: 0,
            created_at: now,
        };

        self.session.plan = Some(plan.clone());
        AgentSessionOperations::save_agent_plan(&self.db, &self.session.id, &plan)
            .map_err(|e| e.to_string())?;
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_PLAN_CREATED,
            json!({
                "session_id": self.session.id,
                "plan": plan,
            }),
            Utc::now().timestamp_millis(),
        ));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_flat_step<F>(
        &mut self,
        _call_llm: &mut F,
        effective_type: &str,
        description: Option<String>,
        tool: Option<String>,
        tools: Option<Vec<ControllerToolCallSpec>>,
        args: Value,
        output_mode: Option<String>,
        message: Option<String>,
        question: Option<String>,
        context: Option<String>,
        resume_to: Option<ResumeTarget>,
    ) -> Result<StepExecutionOutcome, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        self.tool_calls_in_current_step = 0;
        let step_description = description
            .clone()
            .unwrap_or_else(|| default_step_description(effective_type).to_string());
        let plan = self.session.plan.as_mut().ok_or("Missing plan")?;
        let step_id = format!("step-{}", Uuid::new_v4());
        let sequence = plan.steps.len();
        let expected_outcome = "Step result recorded.".to_string();
        let action = match effective_type {
            "tool" => StepAction::ToolCall {
                tool: tool.clone().unwrap_or_default(),
                args: normalize_tool_args(args.clone()),
            },
            "tool_batch" => StepAction::ToolBatch {
                tools: tools
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|entry| ToolBatchToolCall {
                        tool: entry.tool,
                        args: normalize_tool_args(entry.args),
                        output_mode: entry.output_mode,
                    })
                    .collect(),
            },
            "respond" => StepAction::Respond {
                message: message.clone().unwrap_or_default(),
            },
            "ask_user" => StepAction::AskUser {
                question: question.clone().unwrap_or_default(),
            },
            _ => {
                return Err(format!("Unknown step type: {effective_type}"));
            }
        };

        let plan_step = PlanStep {
            id: step_id.clone(),
            sequence,
            description: step_description,
            expected_outcome,
            action,
            status: StepStatus::Proposed,
            result: None,
            approval: None,
        };

        plan.steps.push(plan_step.clone());
        AgentSessionOperations::save_plan_steps(&self.db, &plan.id, &[plan_step.clone()])
            .map_err(|e| e.to_string())?;

        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_PLAN_ADJUSTED,
            json!({
                "session_id": self.session.id,
                "plan": plan.clone(),
            }),
            Utc::now().timestamp_millis(),
        ));

        let preview = if effective_type == "tool" {
            tool.as_deref().and_then(|tool_name| {
                self.tool_registry
                    .get(tool_name)
                    .and_then(|tool_def| tool_def.preview.as_ref())
                    .and_then(|preview_fn| {
                        preview_fn(normalize_tool_args(args.clone()), ToolExecutionContext).ok()
                    })
            })
        } else {
            None
        };

        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_STEP_PROPOSED,
            json!({
                "session_id": self.session.id,
                "step": plan_step,
                "risk": "None",
                "approval_id": null,
                "preview": preview,
            }),
            Utc::now().timestamp_millis(),
        ));

        self.set_phase(PhaseKind::Executing {
            step_id: step_id.clone(),
            tool_iteration: 0,
        })?;
        self.update_step_status(&step_id, StepStatus::Executing)?;
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_STEP_STARTED,
            json!({
                "session_id": self.session.id,
                "step_id": step_id.clone(),
            }),
            Utc::now().timestamp_millis(),
        ));

        let is_respond = effective_type == "respond";
        let respond_message = if is_respond {
            message.clone()
        } else if effective_type == "ask_user" {
            question.clone()
        } else {
            None
        };
        let ask_user_payload = if effective_type == "ask_user" {
            Some((
                question.unwrap_or_default(),
                context,
                resume_to.unwrap_or_else(default_resume_target),
            ))
        } else {
            None
        };

        let result = match effective_type {
            "tool" => {
                let tool_name = tool.unwrap_or_default();
                let requested_output_mode = parse_output_mode_hint(output_mode.as_deref())?;
                self.execute_tool(
                    &step_id,
                    &tool_name,
                    normalize_tool_args(args),
                    requested_output_mode,
                )?
            }
            "tool_batch" => {
                let calls = tools.unwrap_or_default();
                self.execute_tool_batch(&step_id, calls)?
            }
            "respond" => StepResult {
                step_id: step_id.clone(),
                success: true,
                output: Some(json!({ "message": message.unwrap_or_default() })),
                error: None,
                tool_executions: Vec::new(),
                duration_ms: 0,
                completed_at: Utc::now(),
            },
            "ask_user" => StepResult {
                step_id: step_id.clone(),
                success: true,
                output: Some(
                    json!({ "question": ask_user_payload.as_ref().map(|(q, _, _)| q.clone()).unwrap_or_default() }),
                ),
                error: None,
                tool_executions: Vec::new(),
                duration_ms: 0,
                completed_at: Utc::now(),
            },
            _ => unreachable!(),
        };

        let status = if result.success {
            StepStatus::Completed
        } else {
            StepStatus::Failed
        };
        if let Some(step) = self
            .session
            .plan
            .as_mut()
            .and_then(|plan| plan.steps.iter_mut().find(|s| s.id == step_id))
        {
            step.status = status.clone();
            step.result = Some(result.clone());
        }
        self.update_step_status(&step_id, status.clone())?;
        AgentSessionOperations::save_step_result(&self.db, &result).map_err(|e| e.to_string())?;

        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_STEP_COMPLETED,
            json!({
                "session_id": self.session.id,
                "step_id": step_id.clone(),
                "success": result.success,
                "result": result.output.clone(),
                "error": result.error.clone(),
            }),
            Utc::now().timestamp_millis(),
        ));

        let result_error = result.error.clone();
        self.last_step_result = Some(result.clone());
        self.session.step_results.push(result);
        self.append_tool_result_message();
        if ask_user_payload.is_none() {
            self.set_phase(PhaseKind::Controller)?;
        }

        if let Some(error) = result_error.as_deref() {
            if error == "Tool execution denied by approval"
                || error == "Tool approval timed out"
                || error == "Tool execution cancelled"
            {
                return Ok(StepExecutionOutcome::Complete(
                    "Okay, stopping since the tool request wasn't approved. Let me know how you'd like to continue."
                        .to_string(),
                ));
            }
        }

        if let Some((question, context, resume_to)) = ask_user_payload {
            self.requested_user_input = true;
            let _ = context;
            let _ = resume_to;
            return Ok(StepExecutionOutcome::Complete(
                respond_message.unwrap_or(question),
            ));
        }

        if is_respond {
            return Ok(StepExecutionOutcome::Complete(
                respond_message.unwrap_or_default(),
            ));
        }

        Ok(StepExecutionOutcome::Continue)
    }

    fn resolve_requires_approval(&self, tool_name: &str, default_requires_approval: bool) -> bool {
        match get_conversation_tool_approval_override(
            &self.db,
            &self.session.conversation_id,
            tool_name,
        ) {
            Ok(Some(value)) => value,
            Ok(None) => match get_tool_approval_override(&self.db, tool_name) {
                Ok(Some(value)) => value,
                Ok(None) => default_requires_approval,
                Err(err) => {
                    log::warn!(
                        "Failed to load global tool approval override for {}: {}",
                        tool_name,
                        err
                    );
                    default_requires_approval
                }
            },
            Err(err) => {
                log::warn!(
                    "Failed to load conversation tool approval override for {}: {}",
                    tool_name,
                    err
                );
                default_requires_approval
            }
        }
    }

    fn execute_tool(
        &mut self,
        step_id: &str,
        tool_name: &str,
        args: Value,
        requested_output_mode: OutputModeHint,
    ) -> Result<StepResult, String> {
        if self.tool_calls_in_current_step >= self.session.config.max_tool_calls_per_step {
            return Err("Exceeded tool call limit".to_string());
        }
        let iteration = self.tool_calls_in_current_step + 1;
        self.set_phase(PhaseKind::Executing {
            step_id: step_id.to_string(),
            tool_iteration: iteration,
        })?;

        let args = hydrate_tool_args_for_execution(
            tool_name,
            args,
            &self.session.conversation_id,
            self.last_step_result.as_ref(),
            &self.session.step_results,
        );

        let tool = match self.tool_registry.get(tool_name) {
            Some(tool) => tool,
            None => {
                return Ok(self.build_preflight_failed_step_result(
                    step_id,
                    tool_name,
                    args,
                    iteration,
                    format!("Unknown tool: {tool_name}"),
                ));
            }
        };
        if let Err(err) = self.tool_registry.validate_args(&tool.metadata, &args) {
            return Ok(self.build_preflight_failed_step_result(
                step_id,
                tool_name,
                args,
                iteration,
                err.message,
            ));
        }
        if let Err(err) = validate_tool_execution_preflight(tool_name, &args) {
            return Ok(self.build_preflight_failed_step_result(
                step_id, tool_name, args, iteration, err,
            ));
        }

        let execution_id = Uuid::new_v4().to_string();
        let mut tool_executions = Vec::new();
        let requires_approval =
            self.resolve_requires_approval(tool_name, tool.metadata.requires_approval);

        if requires_approval {
            let preview = match tool.preview.as_ref() {
                Some(preview_fn) => Some(
                    preview_fn(args.clone(), ToolExecutionContext).map_err(|err| err.message)?,
                ),
                None => None,
            };
            let timestamp_ms = Utc::now().timestamp_millis();
            let (approval_id, approval_rx) =
                self.approvals.create_request(PendingToolApprovalInput {
                    execution_id: execution_id.clone(),
                    tool_name: tool_name.to_string(),
                    args: args.clone(),
                    preview: preview.clone(),
                    iteration,
                    conversation_id: Some(self.session.conversation_id.clone()),
                    message_id: Some(self.assistant_message_id.clone()),
                    timestamp_ms,
                });
            log::info!(
                "[tool] approval requested: tool={} execution_id={} approval_id={} iteration={} session_id={} conversation_id={} message_id={}",
                tool_name,
                execution_id,
                approval_id,
                iteration,
                self.session.id,
                self.session.conversation_id,
                self.assistant_message_id
            );
            self.event_bus.publish(AgentEvent::new_with_timestamp(
                EVENT_TOOL_EXECUTION_PROPOSED,
                json!({
                    "execution_id": execution_id.clone(),
                    "approval_id": approval_id.clone(),
                    "tool_name": tool_name,
                    "args": args.clone(),
                    "preview": preview,
                    "iteration": iteration,
                    "conversation_id": self.session.conversation_id,
                    "message_id": self.assistant_message_id,
                    "timestamp_ms": timestamp_ms,
                }),
                timestamp_ms,
            ));

            let approval_start = Instant::now();
            let mut forced_denial_reason: Option<&'static str> = None;
            let decision = loop {
                if self.is_cancelled() {
                    let _ = self.approvals.cancel(&approval_id);
                    forced_denial_reason = Some("Tool execution cancelled");
                    break ToolApprovalDecision::Denied;
                }

                match approval_rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(decision) => break decision,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if approval_start.elapsed().as_millis() as u64
                            >= self.session.config.approval_timeout_ms
                        {
                            let _ = self.approvals.cancel(&approval_id);
                            forced_denial_reason = Some("Tool approval timed out");
                            break ToolApprovalDecision::Denied;
                        }
                    }
                    Err(_) => return Err("Approval channel closed".to_string()),
                }
            };

            let timestamp_ms = Utc::now().timestamp_millis();
            match decision {
                ToolApprovalDecision::Approved => {
                    log::info!(
                        "[tool] approval approved: tool={} execution_id={} approval_id={} iteration={} session_id={} conversation_id={} message_id={}",
                        tool_name,
                        execution_id,
                        approval_id,
                        iteration,
                        self.session.id,
                        self.session.conversation_id,
                        self.assistant_message_id
                    );
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_TOOL_EXECUTION_APPROVED,
                        json!({
                            "execution_id": execution_id.clone(),
                            "approval_id": approval_id,
                            "tool_name": tool_name,
                            "iteration": iteration,
                            "conversation_id": self.session.conversation_id,
                            "message_id": self.assistant_message_id,
                            "timestamp_ms": timestamp_ms,
                        }),
                        timestamp_ms,
                    ));
                }
                ToolApprovalDecision::Denied => {
                    let denied_error = forced_denial_reason
                        .unwrap_or("Tool execution denied by approval")
                        .to_string();
                    log::warn!(
                        "[tool] approval denied: tool={} execution_id={} approval_id={} iteration={} session_id={} conversation_id={} message_id={}",
                        tool_name,
                        execution_id,
                        approval_id,
                        iteration,
                        self.session.id,
                        self.session.conversation_id,
                        self.assistant_message_id
                    );
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_TOOL_EXECUTION_DENIED,
                        json!({
                            "execution_id": execution_id,
                            "approval_id": approval_id,
                            "tool_name": tool_name,
                            "iteration": iteration,
                            "conversation_id": self.session.conversation_id,
                            "message_id": self.assistant_message_id,
                            "timestamp_ms": timestamp_ms,
                        }),
                        timestamp_ms,
                    ));
                    tool_executions.push(ToolExecutionRecord {
                        execution_id: execution_id.clone(),
                        tool_name: tool_name.to_string(),
                        args: args.clone(),
                        result: None,
                        success: false,
                        error: Some(denied_error.clone()),
                        duration_ms: 0,
                        iteration: iteration as usize,
                        timestamp_ms,
                        requested_output_mode: Some(requested_output_mode.as_str().to_string()),
                        resolved_output_mode: None,
                        forced_persist: None,
                        forced_reason: None,
                    });
                    self.pending_tool_executions
                        .push(MessageToolExecutionInput {
                            id: execution_id,
                            message_id: self.assistant_message_id.clone(),
                            tool_name: tool_name.to_string(),
                            parameters: args,
                            result: json!(null),
                            success: false,
                            duration_ms: 0,
                            timestamp_ms,
                            error: Some(denied_error.clone()),
                            iteration_number: iteration as i64,
                        });
                    return Ok(StepResult {
                        step_id: step_id.to_string(),
                        success: false,
                        output: None,
                        error: Some(denied_error),
                        tool_executions,
                        duration_ms: 0,
                        completed_at: Utc::now(),
                    });
                }
            }
        }

        if self.is_cancelled() {
            return Err("Cancelled".to_string());
        }

        self.tool_calls_in_current_step += 1;
        let args_summary = summarize_tool_args(&args, 500);
        log::info!(
            "[tool] execution started: tool={} execution_id={} requires_approval={} iteration={} session_id={} conversation_id={} message_id={} args={}",
            tool_name,
            execution_id,
            requires_approval,
            self.tool_calls_in_current_step,
            self.session.id,
            self.session.conversation_id,
            self.assistant_message_id,
            args_summary
        );
        let timestamp_ms = Utc::now().timestamp_millis();
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_TOOL_EXECUTION_STARTED,
            json!({
                "execution_id": execution_id.clone(),
                "tool_name": tool_name,
                "args": args.clone(),
                "requires_approval": requires_approval,
                "iteration": self.tool_calls_in_current_step,
                "conversation_id": self.session.conversation_id,
                "message_id": self.assistant_message_id,
                "timestamp_ms": timestamp_ms,
            }),
            timestamp_ms,
        ));

        let start = Instant::now();
        let result = self.execute_tool_with_timeout(tool, args.clone());
        let duration_ms = start.elapsed().as_millis() as i64;
        let completed_at = Utc::now();
        let timestamp_ms = completed_at.timestamp_millis();
        let mut output_delivery: Option<OutputDeliveryResolution> = None;
        let mut artifact_persist_warning: Option<String> = None;
        let (success, output, error) = match result {
            Ok(output_value) => {
                let output_chars = value_char_len(&output_value);
                let delivery = resolve_output_delivery(
                    tool_name,
                    requested_output_mode,
                    &tool.metadata.result_mode,
                    output_chars,
                );
                output_delivery = Some(delivery.clone());

                let (preview, preview_truncated) =
                    summarize_tool_output_value(&output_value, PERSISTED_RESULT_PREVIEW_MAX_CHARS);
                let metadata = compute_output_metadata(&output_value);
                let should_store_artifact = !tool_name.starts_with("tool_outputs.");

                let (output_ref, persist_error) = if should_store_artifact {
                    let record = ToolOutputRecord {
                        id: execution_id.clone(),
                        tool_name: tool_name.to_string(),
                        conversation_id: Some(self.session.conversation_id.clone()),
                        message_id: self.assistant_message_id.clone(),
                        created_at: timestamp_ms,
                        success: true,
                        parameters: args.clone(),
                        output: output_value.clone(),
                    };

                    match store_tool_output(&record) {
                        Ok(output_ref) => (Some(output_ref), None),
                        Err(err) => (None, Some(format!("Failed to persist tool output: {err}"))),
                    }
                } else {
                    (None, None)
                };

                match delivery.resolved_output_mode {
                    ResolvedOutputMode::Inline => {
                        if let Some(error_message) = persist_error {
                            artifact_persist_warning = Some(error_message.clone());
                            log::warn!(
                                "[tool] artifact persistence warning: tool={} execution_id={} warning={}",
                                tool_name,
                                execution_id,
                                error_message
                            );
                        }
                        (true, Some(output_value), None)
                    }
                    ResolvedOutputMode::Persist => {
                        if let Some(error_message) = persist_error {
                            let message = json!({
                                "message": error_message,
                                "success": false
                            });
                            (false, Some(message), Some(error_message))
                        } else if let Some(output_ref) = output_ref {
                            let message = json!({
                                "persisted": true,
                                "output_ref": output_ref,
                                "size_chars": output_chars as i64,
                                "preview": preview,
                                "preview_truncated": preview_truncated,
                                "metadata": metadata,
                                "requested_output_mode": delivery.requested_output_mode.as_str(),
                                "resolved_output_mode": delivery.resolved_output_mode.as_str(),
                                "forced_persist": delivery.forced_persist,
                                "forced_reason": delivery.forced_reason,
                                "available_tools": [
                                    "tool_outputs.read — load full output into context",
                                    "tool_outputs.extract — extract fields via JSONPath",
                                    "tool_outputs.stats — get schema, field types, counts",
                                    "tool_outputs.count — count items matching criteria",
                                    "tool_outputs.sample — sample items from arrays",
                                    "tool_outputs.list — list all stored outputs"
                                ]
                            });
                            (true, Some(message), None)
                        } else {
                            let error_message =
                                "Resolved persisted output but missing output_ref".to_string();
                            let message = json!({
                                "message": error_message,
                                "success": false
                            });
                            (false, Some(message), Some(error_message))
                        }
                    }
                }
            }
            Err(error_message) => {
                let message = json!({
                    "message": error_message,
                    "success": false
                });
                (false, Some(message), Some(error_message))
            }
        };

        if success {
            let result_for_event = output.clone().unwrap_or_else(|| json!(null));
            let mut payload = json!({
                "execution_id": execution_id.clone(),
                "tool_name": tool_name,
                "result": result_for_event,
                "success": true,
                "duration_ms": duration_ms,
                "iteration": self.tool_calls_in_current_step,
                "conversation_id": self.session.conversation_id,
                "message_id": self.assistant_message_id,
                "timestamp_ms": timestamp_ms,
            });
            if let Some(warning) = artifact_persist_warning.as_ref() {
                payload["artifact_persist_warning"] = Value::String(warning.clone());
            }
            log::info!(
                "[tool] execution completed: tool={} execution_id={} duration_ms={} success=true session_id={} conversation_id={} message_id={}",
                tool_name,
                execution_id,
                duration_ms,
                self.session.id,
                self.session.conversation_id,
                self.assistant_message_id
            );
            self.event_bus.publish(AgentEvent::new_with_timestamp(
                EVENT_TOOL_EXECUTION_COMPLETED,
                payload,
                timestamp_ms,
            ));
        } else {
            let error_message = error
                .clone()
                .unwrap_or_else(|| "Tool execution failed".to_string());
            log::warn!(
                "[tool] execution failed: tool={} execution_id={} duration_ms={} error={} session_id={} conversation_id={} message_id={}",
                tool_name,
                execution_id,
                duration_ms,
                error_message,
                self.session.id,
                self.session.conversation_id,
                self.assistant_message_id
            );
            self.event_bus.publish(AgentEvent::new_with_timestamp(
                EVENT_TOOL_EXECUTION_COMPLETED,
                json!({
                    "execution_id": execution_id.clone(),
                    "tool_name": tool_name,
                    "success": false,
                    "error": error_message,
                    "duration_ms": duration_ms,
                    "iteration": self.tool_calls_in_current_step,
                    "conversation_id": self.session.conversation_id,
                    "message_id": self.assistant_message_id,
                    "timestamp_ms": timestamp_ms,
                }),
                timestamp_ms,
            ));
        }

        tool_executions.push(ToolExecutionRecord {
            execution_id: execution_id.clone(),
            tool_name: tool_name.to_string(),
            args: args.clone(),
            result: output.clone(),
            success,
            error: error.clone(),
            duration_ms,
            iteration: self.tool_calls_in_current_step as usize,
            timestamp_ms,
            requested_output_mode: Some(requested_output_mode.as_str().to_string()),
            resolved_output_mode: output_delivery
                .as_ref()
                .map(|delivery| delivery.resolved_output_mode.as_str().to_string()),
            forced_persist: output_delivery
                .as_ref()
                .map(|delivery| delivery.forced_persist),
            forced_reason: output_delivery
                .as_ref()
                .and_then(|delivery| delivery.forced_reason.map(str::to_string)),
        });

        self.pending_tool_executions
            .push(MessageToolExecutionInput {
                id: execution_id,
                message_id: self.assistant_message_id.clone(),
                tool_name: tool_name.to_string(),
                parameters: args,
                result: output.clone().unwrap_or_else(|| json!(null)),
                success,
                duration_ms,
                timestamp_ms,
                error: error.clone(),
                iteration_number: self.tool_calls_in_current_step as i64,
            });

        Ok(StepResult {
            step_id: step_id.to_string(),
            success,
            output,
            error,
            tool_executions,
            duration_ms,
            completed_at,
        })
    }

    fn execute_tool_batch(
        &mut self,
        step_id: &str,
        calls: Vec<ControllerToolCallSpec>,
    ) -> Result<StepResult, String> {
        let completed_at = Utc::now();
        let requested_calls = calls.len();

        if calls.is_empty() {
            let error = "tool_batch requires at least one tool call".to_string();
            return Ok(StepResult {
                step_id: step_id.to_string(),
                success: false,
                output: Some(json!({
                    "success": false,
                    "message": error.clone()
                })),
                error: Some(error),
                tool_executions: Vec::new(),
                duration_ms: 0,
                completed_at,
            });
        }

        let remaining_capacity = self
            .session
            .config
            .max_tool_calls_per_step
            .saturating_sub(self.tool_calls_in_current_step);
        if remaining_capacity == 0 {
            let error = format!(
                "tool_batch requested {} calls but only {} tool calls remain in this step",
                requested_calls,
                remaining_capacity
            );
            return Ok(StepResult {
                step_id: step_id.to_string(),
                success: false,
                output: Some(json!({
                    "success": false,
                    "message": error.clone(),
                    "requested_calls": requested_calls,
                    "remaining_tool_calls": remaining_capacity
                })),
                error: Some(error),
                tool_executions: Vec::new(),
                duration_ms: 0,
                completed_at,
            });
        }
        let (calls, dropped_calls) =
            clamp_tool_batch_calls_to_remaining_capacity(calls, remaining_capacity);
        if dropped_calls > 0 {
            log::warn!(
                "[tool_batch] requested {} calls but only {} are allowed in one step; executing first {} and dropping {}",
                requested_calls,
                remaining_capacity,
                calls.len(),
                dropped_calls
            );
        }

        let requires_sequential = calls.iter().any(|call| {
            let tool_name = call.tool.trim();
            self.tool_registry.get(tool_name).is_some_and(|tool| {
                self.resolve_requires_approval(tool_name, tool.metadata.requires_approval)
            })
        });
        if requires_sequential {
            log::info!(
                "[tool_batch] approval-required tool detected; executing sequentially for correctness"
            );
            return self.execute_tool_batch_sequential(
                step_id,
                calls,
                requested_calls,
                dropped_calls,
            );
        }

        let started = Instant::now();
        let mut aggregated_tool_executions = Vec::new();
        let mut results_summary = Vec::new();
        let mut first_error: Option<String> = None;
        let mut successful_calls = 0usize;
        let mut runnable_calls = Vec::new();
        let mut iteration_cursor = self.tool_calls_in_current_step + 1;

        for call in calls {
            if self.is_cancelled() {
                return Err("Cancelled".to_string());
            }
            let requested_output_mode = parse_output_mode_hint(call.output_mode.as_deref())?;
            let tool_name = call.tool.trim().to_string();
            let args = hydrate_tool_args_for_execution(
                &tool_name,
                normalize_tool_args(call.args),
                &self.session.conversation_id,
                self.last_step_result.as_ref(),
                &self.session.step_results,
            );

            let iteration = iteration_cursor;
            let tool = match self.tool_registry.get(&tool_name) {
                Some(tool) => tool.clone(),
                None => {
                    let failed = self.build_preflight_failed_step_result(
                        step_id,
                        &tool_name,
                        args,
                        iteration,
                        format!("Unknown tool: {tool_name}"),
                    );
                    if let Some(exec) = failed.tool_executions.last() {
                        results_summary.push(build_tool_batch_result_summary(exec));
                        aggregated_tool_executions.push(exec.clone());
                    }
                    if first_error.is_none() {
                        first_error = failed.error;
                    }
                    continue;
                }
            };
            if let Err(err) = self.tool_registry.validate_args(&tool.metadata, &args) {
                let failed = self.build_preflight_failed_step_result(
                    step_id,
                    &tool_name,
                    args,
                    iteration,
                    err.message,
                );
                if let Some(exec) = failed.tool_executions.last() {
                    results_summary.push(build_tool_batch_result_summary(exec));
                    aggregated_tool_executions.push(exec.clone());
                }
                if first_error.is_none() {
                    first_error = failed.error;
                }
                continue;
            }
            if let Err(err) = validate_tool_execution_preflight(&tool_name, &args) {
                let failed = self.build_preflight_failed_step_result(
                    step_id, &tool_name, args, iteration, err,
                );
                if let Some(exec) = failed.tool_executions.last() {
                    results_summary.push(build_tool_batch_result_summary(exec));
                    aggregated_tool_executions.push(exec.clone());
                }
                if first_error.is_none() {
                    first_error = failed.error;
                }
                continue;
            }

            let execution_id = Uuid::new_v4().to_string();
            let args_summary = summarize_tool_args(&args, 500);
            log::info!(
                "[tool] execution started (batch-parallel): tool={} execution_id={} iteration={} session_id={} conversation_id={} message_id={} args={}",
                tool_name,
                execution_id,
                iteration,
                self.session.id,
                self.session.conversation_id,
                self.assistant_message_id,
                args_summary
            );
            let timestamp_ms = Utc::now().timestamp_millis();
            self.event_bus.publish(AgentEvent::new_with_timestamp(
                EVENT_TOOL_EXECUTION_STARTED,
                json!({
                    "execution_id": execution_id.clone(),
                    "tool_name": tool_name,
                    "args": args.clone(),
                    "requires_approval": false,
                    "iteration": iteration,
                    "conversation_id": self.session.conversation_id,
                    "message_id": self.assistant_message_id,
                    "timestamp_ms": timestamp_ms,
                }),
                timestamp_ms,
            ));

            runnable_calls.push(ParallelToolCallInput {
                iteration,
                execution_id,
                tool_name,
                args,
                requested_output_mode,
                tool,
                conversation_id: self.session.conversation_id.clone(),
                message_id: self.assistant_message_id.clone(),
            });
            iteration_cursor += 1;
        }

        let timeout_ms = if self.session.config.tool_execution_timeout_ms == 0 {
            PARALLEL_BATCH_FALLBACK_TIMEOUT_MS
        } else {
            self.session.config.tool_execution_timeout_ms
        };
        log::info!(
            "[tool_batch] running {} tools in parallel with timeout_ms={}",
            runnable_calls.len(),
            timeout_ms
        );
        let mut handles = Vec::new();
        for call in runnable_calls {
            let cancel_flag = self.cancel_flag.clone();
            let call_for_panic = call.clone();
            handles.push(std::thread::spawn(move || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute_parallel_tool_call(call, timeout_ms, cancel_flag)
                }))
                .unwrap_or_else(|_| ParallelToolRunResult::from_panic(call_for_panic))
            }));
        }

        let mut run_results = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(result) => run_results.push(result),
                Err(_) => {
                    first_error.get_or_insert_with(|| {
                        "Parallel tool execution worker panicked".to_string()
                    });
                }
            }
        }
        run_results.sort_by_key(|result| result.iteration);

        for result in run_results {
            self.tool_calls_in_current_step = self.tool_calls_in_current_step.max(result.iteration);
            if result.success {
                let result_for_event = result.output.clone().unwrap_or_else(|| json!(null));
                let mut payload = json!({
                    "execution_id": result.execution_id.clone(),
                    "tool_name": result.tool_name,
                    "result": result_for_event,
                    "success": true,
                    "duration_ms": result.duration_ms,
                    "iteration": result.iteration,
                    "conversation_id": self.session.conversation_id,
                    "message_id": self.assistant_message_id,
                    "timestamp_ms": result.timestamp_ms,
                });
                if let Some(warning) = result.artifact_persist_warning.as_ref() {
                    payload["artifact_persist_warning"] = Value::String(warning.clone());
                }
                self.event_bus.publish(AgentEvent::new_with_timestamp(
                    EVENT_TOOL_EXECUTION_COMPLETED,
                    payload,
                    result.timestamp_ms,
                ));
                successful_calls += 1;
            } else {
                let error_message = result
                    .error
                    .clone()
                    .unwrap_or_else(|| "Tool execution failed".to_string());
                self.event_bus.publish(AgentEvent::new_with_timestamp(
                    EVENT_TOOL_EXECUTION_COMPLETED,
                    json!({
                        "execution_id": result.execution_id.clone(),
                        "tool_name": result.tool_name,
                        "success": false,
                        "error": error_message,
                        "duration_ms": result.duration_ms,
                        "iteration": result.iteration,
                        "conversation_id": self.session.conversation_id,
                        "message_id": self.assistant_message_id,
                        "timestamp_ms": result.timestamp_ms,
                    }),
                    result.timestamp_ms,
                ));
                if first_error.is_none() {
                    first_error = result.error.clone();
                }
            }

            let execution = ToolExecutionRecord {
                execution_id: result.execution_id.clone(),
                tool_name: result.tool_name.clone(),
                args: result.args.clone(),
                result: result.output.clone(),
                success: result.success,
                error: result.error.clone(),
                duration_ms: result.duration_ms,
                iteration: result.iteration as usize,
                timestamp_ms: result.timestamp_ms,
                requested_output_mode: Some(result.requested_output_mode.as_str().to_string()),
                resolved_output_mode: result
                    .output_delivery
                    .as_ref()
                    .map(|delivery| delivery.resolved_output_mode.as_str().to_string()),
                forced_persist: result
                    .output_delivery
                    .as_ref()
                    .map(|delivery| delivery.forced_persist),
                forced_reason: result
                    .output_delivery
                    .as_ref()
                    .and_then(|delivery| delivery.forced_reason.map(str::to_string)),
            };
            results_summary.push(build_tool_batch_result_summary(&execution));
            aggregated_tool_executions.push(execution.clone());

            self.pending_tool_executions
                .push(MessageToolExecutionInput {
                    id: result.execution_id,
                    message_id: self.assistant_message_id.clone(),
                    tool_name: result.tool_name,
                    parameters: result.args,
                    result: result.output.unwrap_or_else(|| json!(null)),
                    success: result.success,
                    duration_ms: result.duration_ms,
                    timestamp_ms: result.timestamp_ms,
                    error: result.error,
                    iteration_number: result.iteration as i64,
                });
        }

        let duration_ms = started.elapsed().as_millis() as i64;
        let total_calls = results_summary.len();
        let success = first_error.is_none();
        let error = first_error;
        let output = Some(json!({
            "success": success,
            "batch_size": total_calls,
            "requested_calls": requested_calls,
            "executed_calls": total_calls,
            "dropped_calls": dropped_calls,
            "successful_calls": successful_calls,
            "failed_calls": total_calls.saturating_sub(successful_calls),
            "execution_mode": "parallel",
            "results": results_summary
        }));

        Ok(StepResult {
            step_id: step_id.to_string(),
            success,
            output,
            error,
            tool_executions: aggregated_tool_executions,
            duration_ms,
            completed_at: Utc::now(),
        })
    }

    fn execute_tool_batch_sequential(
        &mut self,
        step_id: &str,
        calls: Vec<ControllerToolCallSpec>,
        requested_calls: usize,
        dropped_calls: usize,
    ) -> Result<StepResult, String> {
        let started = Instant::now();
        let mut aggregated_tool_executions = Vec::new();
        let mut results_summary = Vec::new();
        let mut first_error: Option<String> = None;
        let mut successful_calls = 0usize;

        for call in calls {
            let requested_output_mode = parse_output_mode_hint(call.output_mode.as_deref())?;
            let tool_name = call.tool.trim().to_string();
            let normalized_args = normalize_tool_args(call.args);
            let call_result =
                self.execute_tool(step_id, &tool_name, normalized_args, requested_output_mode)?;

            if call_result.success {
                successful_calls += 1;
            } else if first_error.is_none() {
                first_error = Some(
                    call_result
                        .error
                        .clone()
                        .unwrap_or_else(|| format!("Tool execution failed: {tool_name}")),
                );
            }

            if let Some(execution) = call_result.tool_executions.last() {
                results_summary.push(build_tool_batch_result_summary(execution));
            }
            aggregated_tool_executions.extend(call_result.tool_executions);
        }

        let duration_ms = started.elapsed().as_millis() as i64;
        let total_calls = results_summary.len();
        let success = first_error.is_none();
        let error = first_error;
        let output = Some(json!({
            "success": success,
            "batch_size": total_calls,
            "requested_calls": requested_calls,
            "executed_calls": total_calls,
            "dropped_calls": dropped_calls,
            "successful_calls": successful_calls,
            "failed_calls": total_calls.saturating_sub(successful_calls),
            "execution_mode": "sequential",
            "results": results_summary
        }));

        Ok(StepResult {
            step_id: step_id.to_string(),
            success,
            output,
            error,
            tool_executions: aggregated_tool_executions,
            duration_ms,
            completed_at: Utc::now(),
        })
    }

    fn build_preflight_failed_step_result(
        &mut self,
        step_id: &str,
        tool_name: &str,
        args: Value,
        iteration: u32,
        error_message: String,
    ) -> StepResult {
        let execution_id = Uuid::new_v4().to_string();
        let timestamp_ms = Utc::now().timestamp_millis();
        let completed_at = Utc::now();
        let output = json!({
            "message": error_message.clone(),
            "success": false
        });
        let args_summary = summarize_tool_args(&args, 500);

        log::warn!(
            "[tool] preflight failed: tool={} execution_id={} iteration={} session_id={} conversation_id={} message_id={} error={} args={}",
            tool_name,
            execution_id,
            iteration,
            self.session.id,
            self.session.conversation_id,
            self.assistant_message_id,
            error_message,
            args_summary
        );
        self.record_trace(
            "tool_preflight_error",
            error_message.clone(),
            iteration as i64,
            Some(json!({
                "tool": tool_name,
                "args_summary": args_summary
            })),
        );
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_TOOL_EXECUTION_COMPLETED,
            json!({
                "execution_id": execution_id.clone(),
                "tool_name": tool_name,
                "success": false,
                "error": error_message.clone(),
                "duration_ms": 0,
                "iteration": iteration,
                "conversation_id": self.session.conversation_id,
                "message_id": self.assistant_message_id,
                "timestamp_ms": timestamp_ms,
            }),
            timestamp_ms,
        ));

        self.pending_tool_executions
            .push(MessageToolExecutionInput {
                id: execution_id.clone(),
                message_id: self.assistant_message_id.clone(),
                tool_name: tool_name.to_string(),
                parameters: args.clone(),
                result: output.clone(),
                success: false,
                duration_ms: 0,
                timestamp_ms,
                error: Some(error_message.clone()),
                iteration_number: iteration as i64,
            });

        StepResult {
            step_id: step_id.to_string(),
            success: false,
            output: Some(output.clone()),
            error: Some(error_message.clone()),
            tool_executions: vec![ToolExecutionRecord {
                execution_id,
                tool_name: tool_name.to_string(),
                args,
                result: Some(output),
                success: false,
                error: Some(error_message),
                duration_ms: 0,
                iteration: iteration as usize,
                timestamp_ms,
                requested_output_mode: None,
                resolved_output_mode: None,
                forced_persist: None,
                forced_reason: None,
            }],
            duration_ms: 0,
            completed_at,
        }
    }

    fn execute_tool_with_timeout(
        &self,
        tool: &crate::tools::ToolDefinition,
        args: Value,
    ) -> Result<Value, String> {
        execute_tool_handler_with_timeout(
            self.cancel_flag.clone(),
            self.session.config.tool_execution_timeout_ms,
            tool.handler.clone(),
            args,
        )
    }

    /// Get compacted history messages for controller context
    /// Maintains consistent message structure to preserve cache validity
    fn get_compacted_history_messages(&self) -> Vec<LlmMessage> {
        compact_history_messages_with_limits(
            &self.messages,
            CONTROLLER_HISTORY_MAX_CHARS,
            CONTROLLER_HISTORY_STABLE_PREFIX_MESSAGES,
            CONTROLLER_HISTORY_RECENT_TAIL_MESSAGES,
        )
    }

    /// Build controller messages as an array for optimal caching
    fn build_controller_messages(&self, tool_list: &str) -> Vec<LlmMessage> {
        let mut messages = Vec::new();

        // 1. System message: base controller instructions (CACHED - static)
        messages.push(LlmMessage {
            role: "system".to_string(),
            content: json!(CONTROLLER_PROMPT_BASE),
        });

        // 2. System message: available tools (CACHED - stable per conversation)
        messages.push(LlmMessage {
            role: "system".to_string(),
            content: json!(format!("AVAILABLE TOOLS (JSON):\n{}", tool_list)),
        });

        // 3. Runtime limits to keep tool batches within hard backend caps.
        messages.push(LlmMessage {
            role: "system".to_string(),
            content: json!(self.render_controller_limits()),
        });

        // 4. Conversation history as individual messages (CACHED prefix)
        // Apply smart compaction to keep cache stable while limiting context size
        let history_messages = self.get_compacted_history_messages();
        messages.extend(history_messages);

        messages
    }

    fn render_controller_limits(&self) -> String {
        format!(
            "LIMITS:\nmax_total_llm_turns={}\nmax_tool_calls_per_step={}\nHard rule: for action=\"next_step\" with type=\"tool_batch\", tools length MUST be <= max_tool_calls_per_step.",
            self.session.config.max_total_llm_turns, self.session.config.max_tool_calls_per_step
        )
    }

    fn call_controller<F>(&mut self, call_llm: &mut F) -> Result<ControllerAction, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        let tool_list = {
            let overrides = load_tool_approval_overrides(&self.db).unwrap_or_default();
            let conversation_overrides =
                load_conversation_tool_approval_overrides(&self.db, &self.session.conversation_id)
                    .unwrap_or_default();
            let mut tools = self.tool_registry.list_metadata();
            tools.retain(|tool| tool.name != "gcal.list_calendars");
            for tool in &mut tools {
                if let Some(value) = conversation_overrides.get(&tool.name) {
                    tool.requires_approval = *value;
                    continue;
                }
                if let Some(value) = overrides.get(&tool.name) {
                    tool.requires_approval = *value;
                }
            }
            serde_json::to_string(&tools).unwrap_or_else(|_| "[]".to_string())
        };

        // Build message array instead of single prompt for better caching
        let messages = self.build_controller_messages(&tool_list);
        let trace_iteration = self.session.step_results.len() as i64 + 1;
        let prompt_payload =
            serde_json::to_string_pretty(&messages).unwrap_or_else(|_| "[]".to_string());
        self.record_trace(
            "controller_prompt",
            prompt_payload,
            trace_iteration,
            Some(json!({
                "messages": messages.len(),
                "available_tools_count": self.tool_registry.list_metadata().len()
            })),
        );
        let response = self.call_llm_json(call_llm, &messages, Some(controller_output_format()))?;
        let response_payload = serde_json::to_string_pretty(&response)
            .unwrap_or_else(|_| response.to_string());
        self.record_trace(
            "controller_output",
            response_payload,
            trace_iteration,
            None,
        );
        parse_controller_action(&response)
    }

    fn call_llm_json<F>(
        &mut self,
        call_llm: &mut F,
        messages: &[LlmMessage],
        output_format: Option<Value>,
    ) -> Result<Value, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        // Pass messages directly (system prompt now in messages array)
        let response = (call_llm)(messages, None, output_format)?;
        let json_text = extract_json(&response.content);
        serde_json::from_str(&json_text).map_err(|err| format!("Invalid JSON: {err}"))
    }

    fn append_tool_result_message(&mut self) {
        let Some(last_result) = self.session.step_results.last() else {
            return;
        };
        if last_result.tool_executions.is_empty() {
            return;
        }

        let blocks: Vec<String> = if last_result.tool_executions.len() > 1 {
            last_result
                .tool_executions
                .iter()
                .map(format_tool_execution_batch_summary_line)
                .collect()
        } else {
            last_result
                .tool_executions
                .iter()
                .map(format_tool_execution_summary_block)
                .collect()
        };

        if blocks.is_empty() {
            return;
        }

        let summary = truncate_with_notice(&blocks.join("\n"), CONTROLLER_TOOL_SUMMARY_MAX_CHARS);
        self.messages.push(LlmMessage {
            role: "user".to_string(),
            content: json!(format!("[Tool executions]\n{summary}")),
        });
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    fn update_step_status(&self, step_id: &str, status: StepStatus) -> Result<(), String> {
        AgentSessionOperations::update_plan_step_status(&self.db, step_id, status)
            .map_err(|e| e.to_string())
    }

    fn set_phase(&mut self, next: PhaseKind) -> Result<(), String> {
        self.session.phase = next.clone();
        self.session.updated_at = Utc::now();
        AgentSessionOperations::update_agent_session_phase(&self.db, &self.session.id, &next)
            .map_err(|e| e.to_string())?;
        self.publish_phase_change(next);
        Ok(())
    }

    fn publish_phase_change(&self, to: PhaseKind) {
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_PHASE_CHANGED,
            json!({
                "session_id": self.session.id,
                "phase": to,
            }),
            Utc::now().timestamp_millis(),
        ));
    }

    pub fn take_tool_executions(&mut self) -> Vec<MessageToolExecutionInput> {
        std::mem::take(&mut self.pending_tool_executions)
    }

    pub fn requested_user_input(&self) -> bool {
        self.requested_user_input
    }
}

#[derive(Clone)]
struct ParallelToolCallInput {
    iteration: u32,
    execution_id: String,
    tool_name: String,
    args: Value,
    requested_output_mode: OutputModeHint,
    tool: ToolDefinition,
    conversation_id: String,
    message_id: String,
}

fn clamp_tool_batch_calls_to_remaining_capacity(
    calls: Vec<ControllerToolCallSpec>,
    remaining_capacity: u32,
) -> (Vec<ControllerToolCallSpec>, usize) {
    let capacity = remaining_capacity as usize;
    if calls.len() <= capacity {
        return (calls, 0);
    }
    let dropped = calls.len() - capacity;
    (calls.into_iter().take(capacity).collect(), dropped)
}

struct ParallelToolRunResult {
    iteration: u32,
    execution_id: String,
    tool_name: String,
    args: Value,
    requested_output_mode: OutputModeHint,
    output_delivery: Option<OutputDeliveryResolution>,
    success: bool,
    output: Option<Value>,
    error: Option<String>,
    duration_ms: i64,
    timestamp_ms: i64,
    artifact_persist_warning: Option<String>,
}

impl ParallelToolRunResult {
    fn from_panic(call: ParallelToolCallInput) -> Self {
        let error_message = "Tool execution panicked".to_string();
        Self {
            iteration: call.iteration,
            execution_id: call.execution_id,
            tool_name: call.tool_name,
            args: call.args,
            requested_output_mode: call.requested_output_mode,
            output_delivery: None,
            success: false,
            output: Some(json!({
                "message": error_message,
                "success": false
            })),
            error: Some("Tool execution panicked".to_string()),
            duration_ms: 0,
            timestamp_ms: Utc::now().timestamp_millis(),
            artifact_persist_warning: None,
        }
    }
}

fn execute_parallel_tool_call(
    call: ParallelToolCallInput,
    timeout_ms: u64,
    cancel_flag: Arc<AtomicBool>,
) -> ParallelToolRunResult {
    let start = Instant::now();
    let mut output_delivery: Option<OutputDeliveryResolution> = None;
    let mut artifact_persist_warning: Option<String> = None;

    let execution_result = execute_tool_handler_with_timeout(
        cancel_flag,
        timeout_ms,
        call.tool.handler.clone(),
        call.args.clone(),
    );

    let (success, output, error) = match execution_result {
        Ok(output_value) => {
            let output_chars = value_char_len(&output_value);
            let delivery = resolve_output_delivery(
                &call.tool_name,
                call.requested_output_mode,
                &call.tool.metadata.result_mode,
                output_chars,
            );
            output_delivery = Some(delivery.clone());

            let (preview, preview_truncated) =
                summarize_tool_output_value(&output_value, PERSISTED_RESULT_PREVIEW_MAX_CHARS);
            let metadata = compute_output_metadata(&output_value);
            let should_store_artifact = !call.tool_name.starts_with("tool_outputs.");

            let (output_ref, persist_error) = if should_store_artifact {
                let record = ToolOutputRecord {
                    id: call.execution_id.clone(),
                    tool_name: call.tool_name.clone(),
                    conversation_id: Some(call.conversation_id.clone()),
                    message_id: call.message_id.clone(),
                    created_at: Utc::now().timestamp_millis(),
                    success: true,
                    parameters: call.args.clone(),
                    output: output_value.clone(),
                };
                match store_tool_output(&record) {
                    Ok(output_ref) => (Some(output_ref), None),
                    Err(err) => (None, Some(format!("Failed to persist tool output: {err}"))),
                }
            } else {
                (None, None)
            };

            match delivery.resolved_output_mode {
                ResolvedOutputMode::Inline => {
                    if let Some(error_message) = persist_error {
                        artifact_persist_warning = Some(error_message.clone());
                        log::warn!(
                            "[tool] artifact persistence warning: tool={} execution_id={} warning={}",
                            call.tool_name,
                            call.execution_id,
                            error_message
                        );
                    }
                    (true, Some(output_value), None)
                }
                ResolvedOutputMode::Persist => {
                    if let Some(error_message) = persist_error {
                        let message = json!({
                            "message": error_message,
                            "success": false
                        });
                        (false, Some(message), Some(error_message))
                    } else if let Some(output_ref) = output_ref {
                        let message = json!({
                            "persisted": true,
                            "output_ref": output_ref,
                            "size_chars": output_chars as i64,
                            "preview": preview,
                            "preview_truncated": preview_truncated,
                            "metadata": metadata,
                            "requested_output_mode": delivery.requested_output_mode.as_str(),
                            "resolved_output_mode": delivery.resolved_output_mode.as_str(),
                            "forced_persist": delivery.forced_persist,
                            "forced_reason": delivery.forced_reason,
                            "available_tools": [
                                "tool_outputs.read — load full output into context",
                                "tool_outputs.extract — extract fields via JSONPath",
                                "tool_outputs.stats — get schema, field types, counts",
                                "tool_outputs.count — count items matching criteria",
                                "tool_outputs.sample — sample items from arrays",
                                "tool_outputs.list — list all stored outputs"
                            ]
                        });
                        (true, Some(message), None)
                    } else {
                        let error_message =
                            "Resolved persisted output but missing output_ref".to_string();
                        let message = json!({
                            "message": error_message,
                            "success": false
                        });
                        (false, Some(message), Some(error_message))
                    }
                }
            }
        }
        Err(error_message) => {
            let message = json!({
                "message": error_message,
                "success": false
            });
            (false, Some(message), Some(error_message))
        }
    };

    ParallelToolRunResult {
        iteration: call.iteration,
        execution_id: call.execution_id,
        tool_name: call.tool_name,
        args: call.args,
        requested_output_mode: call.requested_output_mode,
        output_delivery,
        success,
        output,
        error,
        duration_ms: start.elapsed().as_millis() as i64,
        timestamp_ms: Utc::now().timestamp_millis(),
        artifact_persist_warning,
    }
}

fn execute_tool_handler_with_timeout(
    cancel_flag: Arc<AtomicBool>,
    timeout_ms: u64,
    handler: Arc<crate::tools::ToolHandler>,
    args: Value,
) -> Result<Value, String> {
    if timeout_ms == 0 {
        return (handler)(args, ToolExecutionContext).map_err(|err| err.message);
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send((handler)(args, ToolExecutionContext));
    });

    let timeout = Duration::from_millis(timeout_ms);
    let started = Instant::now();
    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err("Tool execution cancelled".to_string());
        }

        let elapsed = started.elapsed();
        if elapsed >= timeout {
            return Err(format!("Tool execution timed out after {timeout_ms} ms"));
        }
        let remaining = timeout.saturating_sub(elapsed);
        let wait_for = if remaining > Duration::from_millis(200) {
            Duration::from_millis(200)
        } else {
            remaining
        };

        match rx.recv_timeout(wait_for) {
            Ok(result) => return result.map_err(|err| err.message),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Tool execution worker disconnected".to_string());
            }
        }
    }
}

fn build_tool_batch_result_summary(execution: &ToolExecutionRecord) -> Value {
    let output_ref = execution
        .result
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
        .unwrap_or_else(|| "none".to_string());
    let metadata = execution
        .result
        .as_ref()
        .and_then(|value| value.get("metadata"))
        .cloned()
        .unwrap_or(Value::Null);
    let preview = if execution.success {
        execution
            .result
            .as_ref()
            .and_then(|value| value.get("preview"))
            .and_then(|value| value.as_str())
            .map(|value| truncate_with_notice(value, PERSISTED_RESULT_PREVIEW_MAX_CHARS))
            .unwrap_or_else(|| {
                execution
                    .result
                    .as_ref()
                    .map(|value| {
                        summarize_tool_output_value(value, PERSISTED_RESULT_PREVIEW_MAX_CHARS).0
                    })
                    .unwrap_or_else(|| "none".to_string())
            })
    } else {
        execution
            .error
            .clone()
            .unwrap_or_else(|| "Tool execution failed".to_string())
    };

    json!({
        "tool": execution.tool_name,
        "execution_id": execution.execution_id,
        "success": execution.success,
        "requested_output_mode": execution.requested_output_mode,
        "resolved_output_mode": execution.resolved_output_mode,
        "forced_persist": execution.forced_persist,
        "forced_reason": execution.forced_reason,
        "output_ref": output_ref,
        "metadata": metadata,
        "preview": preview,
        "error": execution.error
    })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ControllerAction {
    NextStep {
        thinking: Value,
        #[serde(rename = "type")]
        step_type: Option<String>,
        description: Option<String>,
        // tool fields
        tool: Option<String>,
        tools: Option<Vec<ControllerToolCallSpec>>,
        #[serde(default)]
        args: Value,
        output_mode: Option<String>,
        // respond fields
        message: Option<String>,
        // ask_user fields (when type=ask_user inside next_step)
        question: Option<String>,
        context: Option<String>,
        resume_to: Option<ResumeTarget>,
    },
    Complete {
        message: String,
    },
    GuardrailStop {
        reason: String,
        message: Option<String>,
    },
    AskUser {
        question: String,
        #[serde(default)]
        context: Option<String>,
        #[serde(default = "default_resume_target")]
        resume_to: ResumeTarget,
    },
}

#[derive(Clone, Debug, Deserialize)]
struct ControllerToolCallSpec {
    tool: String,
    #[serde(default)]
    args: Value,
    output_mode: Option<String>,
}

impl ControllerAction {
    fn validate(&self) -> Result<(), String> {
        match self {
            ControllerAction::NextStep {
                step_type,
                tool,
                tools,
                message,
                question,
                output_mode,
                ..
            } => {
                let effective_type = step_type
                    .as_deref()
                    .or_else(|| infer_step_type_flat(tool, tools, message, question));
                match effective_type {
                    Some("tool") => {
                        if tool.as_ref().map_or(true, |t| t.trim().is_empty()) {
                            return Err(
                                "next_step type=tool requires non-empty 'tool' field".into()
                            );
                        }
                        if let Some(mode) = output_mode.as_deref() {
                            if OutputModeHint::parse(mode).is_none() {
                                return Err(format!(
                                    "Invalid output_mode '{mode}': expected one of auto, inline, persist"
                                ));
                            }
                        }
                    }
                    Some("tool_batch") => {
                        let entries = tools.as_ref().ok_or_else(|| {
                            "next_step type=tool_batch requires non-empty 'tools' field".to_string()
                        })?;
                        if entries.is_empty() {
                            return Err(
                                "next_step type=tool_batch requires non-empty 'tools' field"
                                    .to_string(),
                            );
                        }
                        for (idx, entry) in entries.iter().enumerate() {
                            if entry.tool.trim().is_empty() {
                                return Err(format!(
                                    "next_step type=tool_batch requires non-empty tool name at tools[{idx}]"
                                ));
                            }
                            if let Some(mode) = entry.output_mode.as_deref() {
                                if OutputModeHint::parse(mode).is_none() {
                                    return Err(format!(
                                        "Invalid output_mode '{mode}' at tools[{idx}]: expected one of auto, inline, persist"
                                    ));
                                }
                            }
                        }
                    }
                    Some("respond") => {
                        if message.as_ref().map_or(true, |m| m.trim().is_empty()) {
                            return Err(
                                "next_step type=respond requires non-empty 'message' field".into(),
                            );
                        }
                    }
                    Some("ask_user") => {
                        if question.as_ref().map_or(true, |q| q.trim().is_empty()) {
                            return Err(
                                "next_step type=ask_user requires non-empty 'question' field"
                                    .into(),
                            );
                        }
                    }
                    None => return Err(
                        "Cannot determine step type: provide 'type' or 'tool'/'message'/'question'"
                            .into(),
                    ),
                    Some(other) => return Err(format!("Unknown step type: {other}")),
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

fn infer_step_type_flat(
    tool: &Option<String>,
    tools: &Option<Vec<ControllerToolCallSpec>>,
    message: &Option<String>,
    question: &Option<String>,
) -> Option<&'static str> {
    if tools.as_ref().is_some_and(|entries| !entries.is_empty()) {
        return Some("tool_batch");
    }
    if tool.as_ref().is_some_and(|t| !t.trim().is_empty()) {
        return Some("tool");
    }
    if question.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        return Some("ask_user");
    }
    if message.as_ref().is_some_and(|m| !m.trim().is_empty()) {
        return Some("respond");
    }
    None
}

enum StepExecutionOutcome {
    Continue,
    Complete(String),
}

fn default_resume_target() -> ResumeTarget {
    ResumeTarget::Reflecting
}

fn parse_controller_action(value: &Value) -> Result<ControllerAction, String> {
    // Step 1: Normalize aliases at the Value level before serde
    let normalized = normalize_controller_value(value);

    // Step 2: Try serde deserialization
    match serde_json::from_value::<ControllerAction>(normalized.clone()) {
        Ok(action) => {
            action.validate()?;
            Ok(action)
        }
        Err(serde_err) => {
            // Step 3: Handle action="respond" -> Complete
            let action_str = normalized.get("action").and_then(|v| v.as_str());
            if action_str == Some("respond") {
                if let Some(msg) = non_empty_string_field(&normalized, &["message", "response"]) {
                    return Ok(ControllerAction::Complete { message: msg });
                }
            }

            // Step 4: Fail with clear error -- no synthesis
            Err(format!("Invalid controller output: {serde_err}"))
        }
    }
}

fn normalize_controller_value(value: &Value) -> Value {
    let Value::Object(map) = value else {
        return value.clone();
    };
    let mut out = map.clone();

    // Hoist nested step fields to top level (backwards compat for LLMs that still nest)
    if let Some(Value::Object(step)) = out.remove("step").or_else(|| out.remove("next_step")) {
        for (key, val) in step {
            out.entry(key).or_insert(val);
        }
    }

    // Normalize tool name aliases
    for alias in ["tool_name", "name"] {
        if out.get("tool").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("tool".to_string(), val);
            }
        }
    }

    // Normalize args aliases
    for alias in ["tool_args", "arguments", "tool_input"] {
        if out.get("args").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("args".to_string(), val);
            }
        }
    }

    // Normalize tool batch aliases
    for alias in ["tool_calls", "calls"] {
        if out.get("tools").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("tools".to_string(), val);
            }
        }
    }
    if let Some(Value::Array(entries)) = out.get_mut("tools") {
        for entry in entries {
            if let Value::Object(tool_entry) = entry {
                for alias in ["tool_name", "name"] {
                    if tool_entry.get("tool").is_none() {
                        if let Some(val) = tool_entry.remove(alias) {
                            tool_entry.insert("tool".to_string(), val);
                        }
                    }
                }
                for alias in ["tool_args", "arguments", "tool_input"] {
                    if tool_entry.get("args").is_none() {
                        if let Some(val) = tool_entry.remove(alias) {
                            tool_entry.insert("args".to_string(), val);
                        }
                    }
                }
            }
        }
    }

    // Normalize message aliases
    for alias in ["response", "content"] {
        if out.get("message").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("message".to_string(), val);
            }
        }
    }

    Value::Object(out)
}

fn default_step_description(step_type: &str) -> &'static str {
    match step_type {
        "tool" => "Call the selected tool",
        "tool_batch" => "Execute a batch of tool calls",
        "respond" => "Respond to the user",
        "ask_user" => "Ask the user for clarification",
        _ => "Continue with the next step",
    }
}

fn non_empty_string_field(root: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        root.get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
    })
}

fn is_blank_string_value(value: &Value) -> bool {
    value
        .as_str()
        .map(|text| text.trim().is_empty())
        .unwrap_or(false)
}

fn controller_output_format() -> Value {
    json_schema_output_format(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["action"],
        "properties": {
            "action": {
                "type": "string",
                "enum": ["next_step", "complete", "guardrail_stop", "ask_user"]
            },
            "thinking": {
                "type": "object",
                "properties": {
                    "task": { "type": "string" },
                    "facts": { "type": "array", "items": { "type": "string" } },
                    "decisions": { "type": "array", "items": { "type": "string" } },
                    "risks": { "type": "array", "items": { "type": "string" } },
                    "confidence": { "type": "number" }
                },
                "additionalProperties": true
            },
            "type": {
                "type": "string",
                "enum": ["tool", "tool_batch", "respond", "ask_user"]
            },
            "description": { "type": "string" },
            "tool": { "type": "string" },
            "tools": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "tool": { "type": "string" },
                        "args": { "type": "string" },
                        "output_mode": {
                            "type": "string",
                            "enum": ["auto", "inline", "persist"]
                        }
                    },
                    "required": ["tool"],
                    "additionalProperties": false
                }
            },
            // NOTE: Anthropic structured outputs enforce `additionalProperties: false` on object schemas.
            // If we typed this as `object` without explicit properties/patternProperties, the LLM would be
            // forced to emit `{}` for args. Encode tool args as JSON text and parse via `normalize_tool_args`.
            "args": { "type": "string" },
            "output_mode": {
                "type": "string",
                "enum": ["auto", "inline", "persist"]
            },
            "message": { "type": "string" },
            "reason": { "type": "string" },
            "question": { "type": "string" },
            "context": { "type": "string" },
            "resume_to": {
                "type": "string",
                "enum": ["reflecting", "controller"]
            }
        },
        "additionalProperties": false
    }))
}

fn summarize_goal(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return "Agent task".to_string();
    }
    let mut result = String::new();
    for ch in trimmed.chars().take(160) {
        result.push(ch);
    }
    result
}

fn normalize_tool_args(args: Value) -> Value {
    match args {
        Value::Null => json!({}),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return json!({});
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(value) if value.is_object() => value,
                Ok(value) => json!({ "value": value }),
                Err(_) => json!({ "input": text }),
            }
        }
        other => other,
    }
}

fn parse_output_mode_hint(value: Option<&str>) -> Result<OutputModeHint, String> {
    match value {
        None => Ok(OutputModeHint::Auto),
        Some(raw) => OutputModeHint::parse(raw).ok_or_else(|| {
            format!("Invalid output_mode '{raw}': expected one of auto, inline, persist")
        }),
    }
}

fn resolve_output_delivery(
    tool_name: &str,
    requested_output_mode: OutputModeHint,
    result_mode: &ToolResultMode,
    output_chars: usize,
) -> OutputDeliveryResolution {
    if tool_name.starts_with("tool_outputs.") {
        return OutputDeliveryResolution {
            requested_output_mode,
            resolved_output_mode: ResolvedOutputMode::Inline,
            forced_persist: false,
            forced_reason: None,
        };
    }

    match requested_output_mode {
        OutputModeHint::Persist => OutputDeliveryResolution {
            requested_output_mode,
            resolved_output_mode: ResolvedOutputMode::Persist,
            forced_persist: false,
            forced_reason: None,
        },
        OutputModeHint::Inline => {
            if output_chars > INLINE_RESULT_HARD_MAX_CHARS {
                OutputDeliveryResolution {
                    requested_output_mode,
                    resolved_output_mode: ResolvedOutputMode::Persist,
                    forced_persist: true,
                    forced_reason: Some("inline_size_exceeds_hard_limit"),
                }
            } else {
                OutputDeliveryResolution {
                    requested_output_mode,
                    resolved_output_mode: ResolvedOutputMode::Inline,
                    forced_persist: false,
                    forced_reason: None,
                }
            }
        }
        OutputModeHint::Auto => match result_mode {
            _ => {
                let should_persist =
                    should_persist_tool_output(tool_name, result_mode, output_chars);
                let forced_persist =
                    matches!(result_mode, ToolResultMode::Inline) && should_persist;
                OutputDeliveryResolution {
                    requested_output_mode,
                    resolved_output_mode: if should_persist {
                        ResolvedOutputMode::Persist
                    } else {
                        ResolvedOutputMode::Inline
                    },
                    forced_persist,
                    forced_reason: if forced_persist {
                        Some("inline_size_exceeds_hard_limit")
                    } else {
                        None
                    },
                }
            }
        },
    }
}

fn hydrate_tool_args_for_execution(
    tool_name: &str,
    args: Value,
    conversation_id: &str,
    last_step_result: Option<&StepResult>,
    history: &[StepResult],
) -> Value {
    if !tool_name.starts_with("tool_outputs.") {
        return args;
    }

    let mut args = normalize_tool_args(args);
    apply_tool_output_arg_defaults(tool_name, &mut args);

    if !tool_outputs_tool_supports_id_hydration(tool_name)
        || value_has_non_empty_string_field(&args, "id")
    {
        return args;
    }

    let output_id = last_step_result
        .and_then(step_result_output_ref_id)
        .or_else(|| history.iter().rev().find_map(step_result_output_ref_id));

    let Some(output_id) = output_id else {
        return args;
    };

    match &mut args {
        Value::Object(map) => {
            map.insert("id".to_string(), Value::String(output_id));
            if tool_outputs_tool_supports_conversation_id(tool_name) {
                let conversation_missing_or_blank = map
                    .get("conversation_id")
                    .map(is_blank_string_value)
                    .unwrap_or(true);
                if conversation_missing_or_blank {
                    map.insert(
                        "conversation_id".to_string(),
                        Value::String(conversation_id.to_string()),
                    );
                }
            }
        }
        _ => {
            args = json!({ "id": output_id });
            if tool_outputs_tool_supports_conversation_id(tool_name) {
                args["conversation_id"] = Value::String(conversation_id.to_string());
            }
        }
    }

    apply_tool_output_arg_defaults(tool_name, &mut args);
    args
}

fn tool_outputs_tool_supports_id_hydration(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "tool_outputs.read"
            | "tool_outputs.stats"
            | "tool_outputs.extract"
            | "tool_outputs.count"
            | "tool_outputs.sample"
    )
}

fn tool_outputs_tool_supports_conversation_id(tool_name: &str) -> bool {
    matches!(tool_name, "tool_outputs.read")
}

fn apply_tool_output_arg_defaults(tool_name: &str, args: &mut Value) {
    if tool_name == "tool_outputs.extract" {
        ensure_extract_paths_default(args);
    }
}

fn ensure_extract_paths_default(args: &mut Value) {
    if !args.is_object() {
        *args = json!({});
    }

    let Some(map) = args.as_object_mut() else {
        return;
    };

    let default_paths = match map.get("paths") {
        Some(Value::Array(values)) if !values.is_empty() => None,
        Some(Value::String(path)) => {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                Some(vec![Value::String("$".to_string())])
            } else {
                Some(vec![Value::String(trimmed.to_string())])
            }
        }
        _ => Some(vec![Value::String("$".to_string())]),
    };

    if let Some(paths) = default_paths {
        map.insert("paths".to_string(), Value::Array(paths));
    }
}

fn validate_tool_execution_preflight(tool_name: &str, args: &Value) -> Result<(), String> {
    validate_tool_outputs_reference_id(tool_name, args)
}

fn validate_tool_outputs_reference_id(tool_name: &str, args: &Value) -> Result<(), String> {
    if !tool_outputs_tool_supports_id_hydration(tool_name) {
        return Ok(());
    }

    let Some(id) = args
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    match tool_output_exists(id) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "Invalid tool_outputs id '{id}': no stored output exists for this id. Use ExecutionId/OutputRef.id from a previous tool execution, or omit id to auto-hydrate from the latest persisted output."
        )),
        Err(err) => Err(format!("Invalid tool_outputs id '{id}': {err}")),
    }
}

fn step_result_output_ref_id(result: &StepResult) -> Option<String> {
    result
        .output
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
}

fn extract_tool_output_ref_id_from_value(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(output_ref_id) = map
                .get("output_ref")
                .and_then(|value| value.get("id"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return Some(output_ref_id.to_string());
            }

            map.values().find_map(extract_tool_output_ref_id_from_value)
        }
        Value::Array(values) => values
            .iter()
            .find_map(extract_tool_output_ref_id_from_value),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|parsed| extract_tool_output_ref_id_from_value(&parsed)),
        _ => None,
    }
}

fn value_has_non_empty_string_field(value: &Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn format_tool_execution_summary_block(exec: &ToolExecutionRecord) -> String {
    let args = summarize_tool_args(&exec.args, CONTROLLER_TOOL_SUMMARY_MAX_ARGS_CHARS);
    let output_ref = exec
        .result
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
        .unwrap_or_else(|| "none".to_string());
    let requested_output_mode = exec
        .requested_output_mode
        .clone()
        .or_else(|| {
            exec.result
                .as_ref()
                .and_then(|value| value.get("requested_output_mode"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "n/a".to_string());
    let resolved_output_mode = exec
        .resolved_output_mode
        .clone()
        .or_else(|| {
            exec.result
                .as_ref()
                .and_then(|value| value.get("resolved_output_mode"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            if output_ref != "none" {
                "persist".to_string()
            } else if exec.success {
                "inline".to_string()
            } else {
                "n/a".to_string()
            }
        });
    let forced_persist = exec.forced_persist.or_else(|| {
        exec.result
            .as_ref()
            .and_then(|value| value.get("forced_persist"))
            .and_then(|value| value.as_bool())
    });
    let forced_reason = exec
        .forced_reason
        .clone()
        .or_else(|| {
            exec.result
                .as_ref()
                .and_then(|value| value.get("forced_reason"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "none".to_string());

    let is_persist = resolved_output_mode == "persist";
    let metadata_value = if let Some(value) = exec
        .result
        .as_ref()
        .and_then(|value| value.get("metadata"))
        .filter(|value| !value.is_null())
    {
        value.clone()
    } else if let Some(value) = exec.result.as_ref() {
        if is_persist {
            Value::Null
        } else {
            compute_output_metadata(value)
        }
    } else {
        Value::Null
    };
    let metadata_value = if is_persist {
        strip_metadata_id_hints(&metadata_value)
    } else {
        metadata_value
    };
    let metadata_summary = if metadata_value.is_null() {
        "none".to_string()
    } else {
        truncate_with_notice(
            &metadata_value.to_string(),
            CONTROLLER_TOOL_SUMMARY_MAX_METADATA_CHARS,
        )
    };

    let mut summary = format!(
        "Tool: {} | ExecutionId: {} | Success: {} | RequestedOutputMode: {} | ResolvedOutputMode: {} | ForcedPersist: {} | ForcedReason: {} | OutputRef: {} | Args: {} | Metadata: {}",
        exec.tool_name,
        exec.execution_id,
        exec.success,
        requested_output_mode,
        resolved_output_mode,
        forced_persist.unwrap_or(false),
        forced_reason,
        output_ref,
        args,
        metadata_summary
    );

    if !exec.success {
        let error = exec.error.as_deref().unwrap_or("Tool execution failed");
        summary.push_str(" | Error: ");
        summary.push_str(&truncate_with_notice(
            error,
            CONTROLLER_TOOL_SUMMARY_MAX_RESULT_CHARS,
        ));
        return summary;
    }

    if is_persist {
        summary.push_str(
            " | Note: Exact values require tool_outputs.extract (omit id to hydrate latest output_ref).",
        );
        return summary;
    }

    let output_json = exec
        .result
        .as_ref()
        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| value.to_string()))
        .unwrap_or_else(|| "none".to_string());
    summary.push_str(" | Output: ");
    summary.push_str(&output_json);
    summary
}

fn format_tool_execution_batch_summary_line(exec: &ToolExecutionRecord) -> String {
    let output_ref = exec
        .result
        .as_ref()
        .and_then(extract_tool_output_ref_id_from_value)
        .unwrap_or_else(|| "none".to_string());
    let error = if exec.success {
        "none".to_string()
    } else {
        truncate_with_notice(
            exec.error.as_deref().unwrap_or("Tool execution failed"),
            CONTROLLER_TOOL_SUMMARY_MAX_RESULT_CHARS / 4,
        )
    };

    format!(
        "Tool: {} | ExecutionId: {} | Success: {} | OutputRef: {} | Error: {}",
        exec.tool_name, exec.execution_id, exec.success, output_ref, error
    )
}

fn summarize_tool_args(args: &Value, max_len: usize) -> String {
    let raw = serde_json::to_string(args).unwrap_or_else(|_| "<invalid-json>".to_string());
    if raw.len() <= max_len {
        return raw;
    }
    let truncated: String = raw.chars().take(max_len).collect();
    format!("{truncated}...")
}

fn strip_metadata_id_hints(metadata: &Value) -> Value {
    match metadata {
        Value::Object(map) => {
            let mut cleaned = map.clone();
            cleaned.remove("id_hints");
            Value::Object(cleaned)
        }
        other => other.clone(),
    }
}

fn should_persist_tool_output(
    tool_name: &str,
    result_mode: &ToolResultMode,
    output_chars: usize,
) -> bool {
    if tool_name.starts_with("tool_outputs.") {
        return false;
    }

    match result_mode {
        ToolResultMode::Inline => output_chars > INLINE_RESULT_HARD_MAX_CHARS,
        ToolResultMode::Persist => true,
        ToolResultMode::Auto => output_chars > AUTO_INLINE_RESULT_MAX_CHARS,
    }
}

fn value_char_len(value: &Value) -> usize {
    serde_json::to_string(value)
        .map(|text| text.chars().count())
        .unwrap_or(usize::MAX)
}

fn summarize_tool_output_value(value: &Value, max_chars: usize) -> (String, bool) {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    truncate_chars(&serialized, max_chars)
}

fn truncate_with_notice(input: &str, max_chars: usize) -> String {
    let (truncated, was_truncated) = truncate_chars(input, max_chars);
    if was_truncated {
        format!("{truncated} ...(truncated)")
    } else {
        truncated
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), !input.is_empty());
    }

    let mut output = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            return (output, true);
        }
        output.push(ch);
    }
    (output, false)
}

fn compact_history_messages_with_limits(
    messages: &[LlmMessage],
    max_chars: usize,
    stable_prefix_messages: usize,
    recent_tail_messages: usize,
) -> Vec<LlmMessage> {
    let message_sizes: Vec<usize> = messages
        .iter()
        .map(|msg| value_to_string(&msg.content).chars().count())
        .collect();
    let total_chars: usize = message_sizes.iter().sum();

    if total_chars <= max_chars {
        return messages.to_vec();
    }

    let prefix_end = messages.len().min(stable_prefix_messages);
    let tail_start = messages.len().saturating_sub(recent_tail_messages);
    if tail_start <= prefix_end {
        return messages.to_vec();
    }

    let mut compacted = Vec::with_capacity(prefix_end + (messages.len() - tail_start));
    compacted.extend_from_slice(&messages[..prefix_end]);
    compacted.extend_from_slice(&messages[tail_start..]);
    compacted
}

fn extract_json(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(marked) = extract_marked_json(trimmed) {
        return marked;
    }
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or("");
    if !first_line.starts_with("```") {
        return trimmed.to_string();
    }

    let mut json_lines: Vec<&str> = lines.collect();
    if let Some(last) = json_lines.last() {
        if last.trim().starts_with("```") {
            json_lines.pop();
        }
    }

    json_lines.join("\n").trim().to_string()
}

fn extract_marked_json(raw: &str) -> Option<String> {
    let start = raw.find(CONTROLLER_JSON_START_MARKER)?;
    let after_start = start + CONTROLLER_JSON_START_MARKER.len();
    let end_relative = raw[after_start..].find(CONTROLLER_JSON_END_MARKER)?;
    let end = after_start + end_relative;
    Some(raw[after_start..end].trim().to_string())
}

fn compute_output_metadata(value: &Value) -> Value {
    let mut metadata = match value {
        Value::Object(map) => {
            let mut sorted_keys = map.keys().cloned().collect::<Vec<_>>();
            sorted_keys.sort();

            let top_level_keys = sorted_keys
                .iter()
                .take(OUTPUT_METADATA_MAX_TOP_LEVEL_KEYS)
                .cloned()
                .collect::<Vec<_>>();
            let top_level_value_types = top_level_keys
                .iter()
                .take(OUTPUT_METADATA_MAX_ITEM_TYPE_HINTS)
                .filter_map(|key| {
                    map.get(key).map(|entry| {
                        json!({
                            "key": key,
                            "type": json_type_name(entry)
                        })
                    })
                })
                .collect::<Vec<_>>();

            json!({
                "root_type": "object",
                "size_chars": value_char_len(value),
                "key_count": map.len(),
                "top_level_keys": top_level_keys,
                "top_level_value_types": top_level_value_types
            })
        }
        Value::Array(arr) => json!({
            "root_type": "array",
            "size_chars": value_char_len(value),
            "array_length": arr.len(),
            "item_type_hints": array_item_type_hints(arr)
        }),
        Value::String(text) => json!({
            "root_type": "string",
            "size_chars": value_char_len(value),
            "string_length": text.chars().count()
        }),
        Value::Number(_) => json!({
            "root_type": "number",
            "size_chars": value_char_len(value)
        }),
        Value::Bool(_) => json!({
            "root_type": "boolean",
            "size_chars": value_char_len(value)
        }),
        Value::Null => json!({
            "root_type": "null",
            "size_chars": value_char_len(value)
        }),
    };

    let mut id_hints = Vec::new();
    collect_id_like_hints(value, "$", 0, &mut id_hints);
    if !id_hints.is_empty() {
        if let Some(object) = metadata.as_object_mut() {
            object.insert("id_hints".to_string(), Value::Array(id_hints));
        }
    }

    bound_output_metadata_size(metadata)
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}

fn array_item_type_hints(values: &[Value]) -> Value {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for item in values.iter().take(OUTPUT_METADATA_SCAN_MAX_ARRAY_ITEMS) {
        let entry = counts.entry(json_type_name(item)).or_insert(0);
        *entry += 1;
    }

    let hints = counts
        .into_iter()
        .take(OUTPUT_METADATA_MAX_ITEM_TYPE_HINTS)
        .map(|(value_type, count)| {
            json!({
                "type": value_type,
                "count": count
            })
        })
        .collect::<Vec<_>>();

    Value::Array(hints)
}

fn collect_id_like_hints(value: &Value, path: &str, depth: usize, hints: &mut Vec<Value>) {
    if depth > OUTPUT_METADATA_SCAN_MAX_DEPTH || hints.len() >= OUTPUT_METADATA_MAX_ID_HINTS {
        return;
    }

    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();

            for key in keys {
                if hints.len() >= OUTPUT_METADATA_MAX_ID_HINTS {
                    break;
                }
                let Some(child) = map.get(&key) else {
                    continue;
                };
                let child_path = format!("{path}.{key}");

                if is_id_like_key(&key) {
                    let mut hint = serde_json::Map::new();
                    hint.insert("path".to_string(), Value::String(child_path.clone()));
                    hint.insert("key".to_string(), Value::String(key.clone()));
                    hint.insert(
                        "value_type".to_string(),
                        Value::String(json_type_name(child).to_string()),
                    );
                    if let Some(sample) = summarize_id_sample(child) {
                        hint.insert("sample".to_string(), Value::String(sample));
                    }
                    hints.push(Value::Object(hint));
                }

                collect_id_like_hints(child, &child_path, depth + 1, hints);
            }
        }
        Value::Array(array) => {
            for (index, child) in array
                .iter()
                .take(OUTPUT_METADATA_SCAN_MAX_ARRAY_ITEMS)
                .enumerate()
            {
                if hints.len() >= OUTPUT_METADATA_MAX_ID_HINTS {
                    break;
                }
                let child_path = format!("{path}[{index}]");
                collect_id_like_hints(child, &child_path, depth + 1, hints);
            }
        }
        _ => {}
    }
}

fn is_id_like_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    normalized == "id" || normalized.ends_with("_id") || normalized.ends_with("id")
}

fn summarize_id_sample(value: &Value) -> Option<String> {
    let raw = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => return None,
    };

    Some(truncate_with_notice(
        &raw,
        OUTPUT_METADATA_MAX_ID_SAMPLE_CHARS,
    ))
}

fn bound_output_metadata_size(mut metadata: Value) -> Value {
    let mut length = serialized_char_len(&metadata);
    if length <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS {
        return metadata;
    }

    if let Some(object) = metadata.as_object_mut() {
        object.remove("id_hints");
        object.insert("metadata_truncated".to_string(), Value::Bool(true));
        object.insert(
            "metadata_truncation_reason".to_string(),
            Value::String("removed_id_hints_for_size_limit".to_string()),
        );
    }

    length = serialized_char_len(&metadata);
    if length <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS {
        return metadata;
    }

    if let Some(object) = metadata.as_object_mut() {
        object.remove("item_type_hints");
        object.remove("top_level_value_types");
        object.insert(
            "metadata_truncation_reason".to_string(),
            Value::String("removed_secondary_hints_for_size_limit".to_string()),
        );
    }

    length = serialized_char_len(&metadata);
    if length <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS {
        return metadata;
    }

    json!({
        "root_type": metadata.get("root_type").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "size_chars": metadata.get("size_chars").cloned().unwrap_or_else(|| Value::from(0)),
        "metadata_truncated": true,
        "metadata_truncation_reason": "hard_size_limit"
    })
}

fn serialized_char_len(value: &Value) -> usize {
    serde_json::to_string(value)
        .map(|serialized| serialized.chars().count())
        .unwrap_or(usize::MAX)
}

fn value_to_string(value: &serde_json::Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(array) = value.as_array() {
        let mut combined = String::new();
        for entry in array {
            if let Some(text) = entry.get("text").and_then(|v| v.as_str()) {
                combined.push_str(text);
            }
        }
        return combined;
    }

    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clamp_tool_batch_calls_drops_overflow_calls() {
        let calls = (0..7)
            .map(|idx| ControllerToolCallSpec {
                tool: format!("tool.{idx}"),
                args: json!({ "idx": idx }),
                output_mode: None,
            })
            .collect::<Vec<_>>();

        let (clamped, dropped) = clamp_tool_batch_calls_to_remaining_capacity(calls, 5);
        assert_eq!(clamped.len(), 5);
        assert_eq!(dropped, 2);
        assert_eq!(clamped[0].tool, "tool.0");
        assert_eq!(clamped[4].tool, "tool.4");
    }

    #[test]
    fn clamp_tool_batch_calls_keeps_batch_when_within_capacity() {
        let calls = (0..3)
            .map(|idx| ControllerToolCallSpec {
                tool: format!("tool.{idx}"),
                args: json!({ "idx": idx }),
                output_mode: None,
            })
            .collect::<Vec<_>>();

        let (clamped, dropped) = clamp_tool_batch_calls_to_remaining_capacity(calls, 5);
        assert_eq!(clamped.len(), 3);
        assert_eq!(dropped, 0);
        assert_eq!(clamped[2].tool, "tool.2");
    }

    #[test]
    fn compacted_history_keeps_prefix_and_tail_without_summary_message() {
        let messages = (0..12)
            .map(|idx| LlmMessage {
                role: "user".to_string(),
                content: json!(format!("message-{idx}-{}", "x".repeat(48))),
            })
            .collect::<Vec<_>>();

        let compacted = compact_history_messages_with_limits(&messages, 200, 2, 3);
        assert_eq!(compacted.len(), 5);
        assert_eq!(
            value_to_string(&compacted[0].content),
            value_to_string(&messages[0].content)
        );
        assert_eq!(
            value_to_string(&compacted[1].content),
            value_to_string(&messages[1].content)
        );
        assert_eq!(
            value_to_string(&compacted[2].content),
            value_to_string(&messages[9].content)
        );
        assert_eq!(
            value_to_string(&compacted[3].content),
            value_to_string(&messages[10].content)
        );
        assert_eq!(
            value_to_string(&compacted[4].content),
            value_to_string(&messages[11].content)
        );
        assert!(
            compacted
                .iter()
                .all(|message| !value_to_string(&message.content).contains("[Context Summary:")),
            "compaction should not inject dynamic summary messages"
        );
    }

    #[test]
    fn extract_json_prefers_marked_envelope() {
        let raw = r#"extra preface
=====JSON_START=====
{
  "action": "complete",
  "message": "ok"
}
=====JSON_END=====
extra suffix"#;

        let extracted = extract_json(raw);
        assert_eq!(
            extracted,
            "{\n  \"action\": \"complete\",\n  \"message\": \"ok\"\n}"
        );
    }

    #[test]
    fn extract_json_falls_back_to_markdown_fence_when_markers_absent() {
        let raw = r#"```json
{
  "action": "complete",
  "message": "ok"
}
```"#;

        let extracted = extract_json(raw);
        assert_eq!(
            extracted,
            "{\n  \"action\": \"complete\",\n  \"message\": \"ok\"\n}"
        );
    }

    #[test]
    fn extract_json_returns_trimmed_raw_when_no_markers_or_fence() {
        let raw = "   {\"action\":\"complete\",\"message\":\"ok\"}   ";
        let extracted = extract_json(raw);
        assert_eq!(extracted, "{\"action\":\"complete\",\"message\":\"ok\"}");
    }

    #[test]
    fn parse_controller_action_accepts_next_step_top_level_tool_payload() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Check weather" },
            "tool": "weather",
            "args": { "location": "Austin, TX" }
        });

        let action = parse_controller_action(&payload).expect("next_step payload should parse");
        match action {
            ControllerAction::NextStep { tool, args, .. } => {
                assert_eq!(tool.as_deref(), Some("weather"));
                assert_eq!(args, json!({ "location": "Austin, TX" }));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_next_step_tool_payload_with_string_args() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Check weather" },
            "tool": "weather",
            "args": "{\"location\":\"Austin, TX\"}"
        });

        let action = parse_controller_action(&payload).expect("payload should parse");
        match action {
            ControllerAction::NextStep { tool, args, .. } => {
                assert_eq!(tool.as_deref(), Some("weather"));
                assert_eq!(
                    normalize_tool_args(args),
                    json!({ "location": "Austin, TX" })
                );
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_next_step_step_hoisted_to_top_level() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Check weather" },
            "step": {
                "tool": "weather",
                "args": { "location": "Austin, TX" }
            }
        });

        let action = parse_controller_action(&payload).expect("step payload should parse");
        match action {
            ControllerAction::NextStep { tool, args, .. } => {
                assert_eq!(tool.as_deref(), Some("weather"));
                assert_eq!(args, json!({ "location": "Austin, TX" }));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_rejects_next_step_with_only_thinking() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Inspect project files before deciding" }
        });

        let result = parse_controller_action(&payload);
        assert!(
            result.is_err(),
            "next_step with only thinking should fail validation (no think synthesis)"
        );
    }

    #[test]
    fn parse_controller_action_rejects_blank_question_without_tool_or_message() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Inspect project files before deciding" },
            "question": "",
            "context": ""
        });

        let result = parse_controller_action(&payload);
        assert!(
            result.is_err(),
            "next_step with blank question and no tool/message should fail"
        );
    }

    #[test]
    fn parse_controller_action_next_step_with_message_infers_respond() {
        let payload = json!({
            "action": "next_step",
            "thinking": {
                "task": "Examine email threads to find meeting information from yesterday",
                "decisions": [
                    "Need to examine individual email threads to find meeting-related content"
                ]
            },
            "message": "I'll examine the email threads to find meeting information from yesterday.",
            "resume_to": "controller"
        });

        let action = parse_controller_action(&payload).expect("payload should parse");
        match action {
            ControllerAction::NextStep {
                step_type, message, ..
            } => {
                // type is inferred as "respond" since message is present
                let effective = step_type
                    .as_deref()
                    .or_else(|| {
                        if message.is_some() {
                            Some("respond")
                        } else {
                            None
                        }
                    })
                    .unwrap();
                assert_eq!(effective, "respond");
                assert_eq!(
                    message.as_deref(),
                    Some("I'll examine the email threads to find meeting information from yesterday.")
                );
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_does_not_synthesize_tool_from_thinking() {
        // With the flat schema, we no longer synthesize tool steps from thinking decisions.
        // The LLM must explicitly provide the tool field.
        let payload = json!({
            "action": "next_step",
            "thinking": {
                "task": "Check the user's last 20 emails to find meetings from yesterday",
                "decisions": [
                    "Use gmail.list_threads to fetch the last 20 email threads"
                ]
            },
            "question": "",
            "context": "",
            "resume_to": "controller"
        });

        let result = parse_controller_action(&payload);
        assert!(
            result.is_err(),
            "next_step without tool/message/question should fail (no synthesis)"
        );
    }

    #[test]
    fn controller_output_schema_has_no_one_of() {
        let schema = controller_output_format();
        let root = schema.get("schema").expect("schema root");

        assert!(root.get("allOf").is_none(), "schema should not have allOf");
        assert!(root.get("oneOf").is_none(), "schema should not have oneOf");
        assert!(root.get("anyOf").is_none(), "schema should not have anyOf");
    }

    #[test]
    fn controller_output_schema_has_flat_type_field() {
        let schema = controller_output_format();
        let root = schema.get("schema").expect("schema root");
        let props = root.get("properties").expect("properties");

        // type is top-level, not nested in step
        assert!(props.get("type").is_some(), "type should be top-level");
        assert!(props.get("step").is_none(), "step should not exist");

        // type enum should not contain "think"
        let type_enum = props.get("type").unwrap().get("enum").expect("type enum");
        let values: Vec<&str> = type_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(values, vec!["tool", "tool_batch", "respond", "ask_user"]);
    }

    #[test]
    fn controller_output_schema_includes_output_mode_enum() {
        let schema = controller_output_format();
        let root = schema.get("schema").expect("schema root");
        let props = root.get("properties").expect("properties");
        let output_mode = props.get("output_mode").expect("output_mode field");
        let values: Vec<&str> = output_mode
            .get("enum")
            .and_then(|enum_values| enum_values.as_array())
            .expect("output_mode enum")
            .iter()
            .filter_map(|value| value.as_str())
            .collect();
        assert_eq!(values, vec!["auto", "inline", "persist"]);
    }

    #[test]
    fn controller_output_schema_includes_tool_batch_shape() {
        let schema = controller_output_format();
        let root = schema.get("schema").expect("schema root");
        let props = root.get("properties").expect("properties");
        let tools = props.get("tools").expect("tools field");
        let items = tools.get("items").expect("tools.items");
        let item_props = items.get("properties").expect("tools.items.properties");

        assert_eq!(
            item_props
                .get("tool")
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str()),
            Some("string")
        );
        assert_eq!(
            item_props
                .get("args")
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str()),
            Some("string")
        );
        assert_eq!(
            items
                .get("additionalProperties")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn parse_controller_action_infers_tool_type_from_tool_field() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Fetch weather data" },
            "tool": "weather.get_forecast",
            "args": { "city": "London" }
        });

        let action = parse_controller_action(&payload).expect("should parse");
        match action {
            ControllerAction::NextStep {
                step_type,
                tool,
                args,
                ..
            } => {
                // type is None because it was inferred, not explicitly set
                assert!(step_type.is_none(), "type should be inferred, not explicit");
                assert_eq!(tool.as_deref(), Some("weather.get_forecast"));
                assert_eq!(args, json!({ "city": "London" }));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_tool_name_alias() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Search files" },
            "tool_name": "files.search",
            "args": { "query": "test" }
        });

        let action = parse_controller_action(&payload).expect("should parse with tool_name alias");
        match action {
            ControllerAction::NextStep { tool, .. } => {
                assert_eq!(tool.as_deref(), Some("files.search"));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_args_aliases() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Read file" },
            "tool": "files.read",
            "arguments": { "path": "/tmp/test.txt" }
        });

        let action = parse_controller_action(&payload).expect("should parse with arguments alias");
        match action {
            ControllerAction::NextStep { tool, args, .. } => {
                assert_eq!(tool.as_deref(), Some("files.read"));
                assert_eq!(args, json!({ "path": "/tmp/test.txt" }));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_handles_respond_action_as_complete() {
        let payload = json!({
            "action": "respond",
            "message": "Here is your answer."
        });

        let action =
            parse_controller_action(&payload).expect("action=respond should map to Complete");
        match action {
            ControllerAction::Complete { message } => {
                assert_eq!(message, "Here is your answer.");
            }
            other => panic!("expected Complete action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_handles_respond_action_with_response_alias() {
        let payload = json!({
            "action": "respond",
            "response": "The result is 42."
        });

        let action = parse_controller_action(&payload)
            .expect("action=respond with response alias should map to Complete");
        match action {
            ControllerAction::Complete { message } => {
                assert_eq!(message, "The result is 42.");
            }
            other => panic!("expected Complete action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_message_alias() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Respond to user" },
            "response": "Here is the info you requested."
        });

        let action =
            parse_controller_action(&payload).expect("response alias should be normalized");
        match action {
            ControllerAction::NextStep { message, .. } => {
                assert_eq!(message.as_deref(), Some("Here is the info you requested."));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_valid_output_mode_with_string_args() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Fetch email threads" },
            "tool": "gmail.list_threads",
            "args": "{\"max_results\":25}",
            "output_mode": "persist"
        });

        let action = parse_controller_action(&payload).expect("payload should parse");
        match action {
            ControllerAction::NextStep {
                tool,
                args,
                output_mode,
                ..
            } => {
                assert_eq!(tool.as_deref(), Some("gmail.list_threads"));
                assert_eq!(normalize_tool_args(args), json!({ "max_results": 25 }));
                assert_eq!(output_mode.as_deref(), Some("persist"));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_rejects_invalid_output_mode() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Fetch email threads" },
            "tool": "gmail.list_threads",
            "args": "{}",
            "output_mode": "fast"
        });

        let result = parse_controller_action(&payload);
        assert!(result.is_err(), "invalid output_mode should be rejected");
    }

    #[test]
    fn parse_controller_action_accepts_tool_batch_with_per_tool_output_mode() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Fetch related resources in one turn" },
            "type": "tool_batch",
            "tools": [
                {
                    "tool": "files.search",
                    "args": "{\"query\":\"orchestrator\"}",
                    "output_mode": "auto"
                },
                {
                    "tool": "files.read_range",
                    "args": "{\"path\":\"src-tauri/src/agent/orchestrator.rs\",\"start_line\":1,\"end_line\":20}",
                    "output_mode": "persist"
                }
            ]
        });

        let action = parse_controller_action(&payload).expect("payload should parse");
        match action {
            ControllerAction::NextStep {
                step_type, tools, ..
            } => {
                assert_eq!(step_type.as_deref(), Some("tool_batch"));
                let tools = tools.expect("tool batch entries");
                assert_eq!(tools.len(), 2);
                assert_eq!(tools[0].tool, "files.search");
                assert_eq!(tools[0].output_mode.as_deref(), Some("auto"));
                assert_eq!(
                    normalize_tool_args(tools[0].args.clone()),
                    json!({ "query": "orchestrator" })
                );
                assert_eq!(tools[1].tool, "files.read_range");
                assert_eq!(tools[1].output_mode.as_deref(), Some("persist"));
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_tool_batch_aliases() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Batch calls using aliases" },
            "type": "tool_batch",
            "tool_calls": [
                {
                    "tool_name": "files.search",
                    "arguments": { "query": "cache_control" }
                }
            ]
        });

        let action = parse_controller_action(&payload).expect("payload should parse");
        match action {
            ControllerAction::NextStep { tools, .. } => {
                let tools = tools.expect("tool batch entries");
                assert_eq!(tools.len(), 1);
                assert_eq!(tools[0].tool, "files.search");
                assert_eq!(
                    normalize_tool_args(tools[0].args.clone()),
                    json!({ "query": "cache_control" })
                );
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_rejects_invalid_output_mode_in_tool_batch_item() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Batch calls" },
            "type": "tool_batch",
            "tools": [
                {
                    "tool": "files.search",
                    "args": "{}",
                    "output_mode": "fast"
                }
            ]
        });

        let result = parse_controller_action(&payload);
        assert!(
            result.is_err(),
            "invalid output_mode in tool batch should be rejected"
        );
    }

    #[test]
    fn hydrate_tool_outputs_read_args_uses_last_step_output_ref() {
        let last_result = StepResult {
            step_id: "step-1".to_string(),
            success: true,
            output: Some(json!({
                "persisted": true,
                "output_ref": {
                    "id": "exec-123",
                    "storage": "app_data"
                }
            })),
            error: None,
            tool_executions: Vec::new(),
            duration_ms: 0,
            completed_at: Utc::now(),
        };

        let hydrated = hydrate_tool_args_for_execution(
            "tool_outputs.read",
            json!({}),
            "conversation-1",
            Some(&last_result),
            &[],
        );

        assert_eq!(
            hydrated.get("id").and_then(|value| value.as_str()),
            Some("exec-123")
        );
        assert_eq!(
            hydrated
                .get("conversation_id")
                .and_then(|value| value.as_str()),
            Some("conversation-1")
        );
    }

    #[test]
    fn hydrate_tool_outputs_read_args_preserves_existing_id() {
        let hydrated = hydrate_tool_args_for_execution(
            "tool_outputs.read",
            json!({ "id": "explicit-id" }),
            "conversation-1",
            None,
            &[],
        );

        assert_eq!(
            hydrated.get("id").and_then(|value| value.as_str()),
            Some("explicit-id")
        );
    }

    #[test]
    fn hydrate_tool_outputs_read_args_uses_history_when_last_result_missing() {
        let history = vec![StepResult {
            step_id: "step-older".to_string(),
            success: true,
            output: Some(json!({
                "result": {
                    "output_ref": {
                        "id": "hist-456"
                    }
                }
            })),
            error: None,
            tool_executions: Vec::new(),
            duration_ms: 0,
            completed_at: Utc::now(),
        }];

        let hydrated = hydrate_tool_args_for_execution(
            "tool_outputs.read",
            json!({}),
            "conversation-1",
            None,
            &history,
        );

        assert_eq!(
            hydrated.get("id").and_then(|value| value.as_str()),
            Some("hist-456")
        );
    }

    #[test]
    fn hydrate_tool_outputs_extract_args_uses_last_step_output_ref_and_default_path() {
        let last_result = StepResult {
            step_id: "step-1".to_string(),
            success: true,
            output: Some(json!({
                "persisted": true,
                "output_ref": {
                    "id": "exec-789",
                    "storage": "app_data"
                }
            })),
            error: None,
            tool_executions: Vec::new(),
            duration_ms: 0,
            completed_at: Utc::now(),
        };

        let hydrated = hydrate_tool_args_for_execution(
            "tool_outputs.extract",
            json!({}),
            "conversation-1",
            Some(&last_result),
            &[],
        );

        assert_eq!(
            hydrated.get("id").and_then(|value| value.as_str()),
            Some("exec-789")
        );
        assert_eq!(hydrated.get("paths"), Some(&json!(["$"])));
        assert!(
            hydrated.get("conversation_id").is_none(),
            "extract schema does not allow conversation_id"
        );
    }

    #[test]
    fn hydrate_tool_outputs_list_args_does_not_inject_id() {
        let last_result = StepResult {
            step_id: "step-1".to_string(),
            success: true,
            output: Some(json!({
                "persisted": true,
                "output_ref": {
                    "id": "exec-list-123",
                    "storage": "app_data"
                }
            })),
            error: None,
            tool_executions: Vec::new(),
            duration_ms: 0,
            completed_at: Utc::now(),
        };

        let hydrated = hydrate_tool_args_for_execution(
            "tool_outputs.list",
            json!({}),
            "conversation-1",
            Some(&last_result),
            &[],
        );

        assert_eq!(hydrated, json!({}));
    }

    #[test]
    fn should_persist_skips_all_tool_outputs_tools() {
        assert!(!should_persist_tool_output(
            "tool_outputs.read",
            &ToolResultMode::Persist,
            1000
        ));
        assert!(!should_persist_tool_output(
            "tool_outputs.extract",
            &ToolResultMode::Persist,
            1000
        ));
        assert!(!should_persist_tool_output(
            "tool_outputs.stats",
            &ToolResultMode::Persist,
            1000
        ));
        assert!(!should_persist_tool_output(
            "tool_outputs.list",
            &ToolResultMode::Persist,
            1000
        ));
        assert!(!should_persist_tool_output(
            "tool_outputs.count",
            &ToolResultMode::Persist,
            1000
        ));
        assert!(!should_persist_tool_output(
            "tool_outputs.sample",
            &ToolResultMode::Persist,
            1000
        ));
    }

    #[test]
    fn resolve_output_delivery_inline_small_stays_inline() {
        let resolution = resolve_output_delivery(
            "gmail.list_threads",
            OutputModeHint::Inline,
            &ToolResultMode::Auto,
            512,
        );
        assert_eq!(resolution.resolved_output_mode, ResolvedOutputMode::Inline);
        assert!(!resolution.forced_persist);
    }

    #[test]
    fn resolve_output_delivery_inline_large_forces_persist() {
        let resolution = resolve_output_delivery(
            "gmail.list_threads",
            OutputModeHint::Inline,
            &ToolResultMode::Auto,
            INLINE_RESULT_HARD_MAX_CHARS + 1,
        );
        assert_eq!(resolution.resolved_output_mode, ResolvedOutputMode::Persist);
        assert!(resolution.forced_persist);
        assert_eq!(
            resolution.forced_reason,
            Some("inline_size_exceeds_hard_limit")
        );
    }

    #[test]
    fn resolve_output_delivery_persist_requested_persists() {
        let resolution = resolve_output_delivery(
            "gmail.list_threads",
            OutputModeHint::Persist,
            &ToolResultMode::Inline,
            20,
        );
        assert_eq!(resolution.resolved_output_mode, ResolvedOutputMode::Persist);
        assert!(!resolution.forced_persist);
    }

    #[test]
    fn resolve_output_delivery_auto_follows_tool_result_mode() {
        let auto_small = resolve_output_delivery(
            "gmail.list_threads",
            OutputModeHint::Auto,
            &ToolResultMode::Auto,
            50,
        );
        assert_eq!(auto_small.resolved_output_mode, ResolvedOutputMode::Inline);

        let auto_large = resolve_output_delivery(
            "gmail.list_threads",
            OutputModeHint::Auto,
            &ToolResultMode::Auto,
            AUTO_INLINE_RESULT_MAX_CHARS + 1,
        );
        assert_eq!(auto_large.resolved_output_mode, ResolvedOutputMode::Persist);

        let force_persist_mode = resolve_output_delivery(
            "gmail.list_threads",
            OutputModeHint::Auto,
            &ToolResultMode::Persist,
            50,
        );
        assert_eq!(
            force_persist_mode.resolved_output_mode,
            ResolvedOutputMode::Persist
        );
    }

    #[test]
    fn resolve_output_delivery_tool_outputs_stays_inline() {
        let resolution = resolve_output_delivery(
            "tool_outputs.read",
            OutputModeHint::Persist,
            &ToolResultMode::Persist,
            100_000,
        );
        assert_eq!(resolution.resolved_output_mode, ResolvedOutputMode::Inline);
        assert!(!resolution.forced_persist);
    }

    #[test]
    fn compute_output_metadata_for_object() {
        let value = json!({ "name": "test", "count": 42, "items": [1, 2, 3] });
        let meta = compute_output_metadata(&value);
        assert_eq!(
            meta.get("root_type").and_then(|v| v.as_str()),
            Some("object")
        );
        assert_eq!(meta.get("key_count").and_then(|v| v.as_u64()), Some(3));
        let keys = meta
            .get("top_level_keys")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn compute_output_metadata_for_array() {
        let value = json!([1, 2, 3, 4, 5]);
        let meta = compute_output_metadata(&value);
        assert_eq!(
            meta.get("root_type").and_then(|v| v.as_str()),
            Some("array")
        );
        assert_eq!(meta.get("array_length").and_then(|v| v.as_u64()), Some(5));
    }

    #[test]
    fn compute_output_metadata_for_string() {
        let value = json!("hello world");
        let meta = compute_output_metadata(&value);
        assert_eq!(
            meta.get("root_type").and_then(|v| v.as_str()),
            Some("string")
        );
    }

    #[test]
    fn compute_output_metadata_includes_rich_bounded_hints() {
        let value = json!({
            "threads": [
                { "id": "thread-1", "message_id": "msg-1" },
                { "id": "thread-2", "message_id": "msg-2" }
            ],
            "next_page_token": "token-123"
        });
        let meta = compute_output_metadata(&value);
        assert_eq!(
            meta.get("root_type").and_then(|v| v.as_str()),
            Some("object")
        );
        assert!(meta.get("size_chars").and_then(|v| v.as_u64()).is_some());
        let hints = meta
            .get("id_hints")
            .and_then(|value| value.as_array())
            .expect("expected id_hints");
        assert!(!hints.is_empty(), "expected id_hints to be populated");
    }

    #[test]
    fn compute_output_metadata_stays_under_size_limit() {
        let mut payload = serde_json::Map::new();
        for idx in 0..256 {
            payload.insert(
                format!("thread_{}_id", idx),
                Value::String(format!("id-{}", "x".repeat(64))),
            );
        }
        let meta = compute_output_metadata(&Value::Object(payload));
        let serialized_len = serialized_char_len(&meta);
        assert!(
            serialized_len <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS,
            "metadata exceeded size limit: {serialized_len}"
        );
    }

    #[test]
    fn format_tool_execution_summary_block_includes_envelope_fields() {
        let exec = ToolExecutionRecord {
            execution_id: "exec-1".to_string(),
            tool_name: "gmail.list_threads".to_string(),
            args: json!({ "max_results": 10 }),
            result: Some(json!({
                "persisted": true,
                "output_ref": { "id": "artifact-123" },
                "preview": "[{\"id\":\"thread-1\"}]",
                "preview_truncated": false,
                "metadata": { "root_type": "array", "array_length": 10 },
                "requested_output_mode": "persist",
                "resolved_output_mode": "persist",
                "forced_persist": false,
                "forced_reason": null
            })),
            success: true,
            error: None,
            duration_ms: 12,
            iteration: 1,
            timestamp_ms: 1_000,
            requested_output_mode: Some("persist".to_string()),
            resolved_output_mode: Some("persist".to_string()),
            forced_persist: Some(false),
            forced_reason: None,
        };

        let summary = format_tool_execution_summary_block(&exec);
        assert!(summary.contains("ExecutionId: exec-1"));
        assert!(summary.contains("RequestedOutputMode: persist"));
        assert!(summary.contains("ResolvedOutputMode: persist"));
        assert!(summary.contains("OutputRef: artifact-123"));
        assert!(!summary.contains("preview"));
        assert!(summary.contains("Exact values require tool_outputs.extract"));
    }

    #[test]
    fn format_tool_execution_summary_block_includes_full_inline_payload() {
        let payload = json!({
            "thread_id": "19c4ca084d0de349",
            "subject": "Hello"
        });
        let exec = ToolExecutionRecord {
            execution_id: "exec-2".to_string(),
            tool_name: "files.read".to_string(),
            args: json!({ "path": "/tmp/notes.txt" }),
            result: Some(payload.clone()),
            success: true,
            error: None,
            duration_ms: 8,
            iteration: 1,
            timestamp_ms: 2_000,
            requested_output_mode: Some("inline".to_string()),
            resolved_output_mode: Some("inline".to_string()),
            forced_persist: Some(false),
            forced_reason: None,
        };

        let summary = format_tool_execution_summary_block(&exec);
        assert!(summary.contains("ResolvedOutputMode: inline"));
        let expected_json = serde_json::to_string(&payload).expect("serialize payload");
        assert!(summary.contains(&expected_json));
    }

    #[test]
    fn format_tool_execution_batch_summary_line_keeps_identity_fields() {
        let exec = ToolExecutionRecord {
            execution_id: "exec-3".to_string(),
            tool_name: "gmail.get_thread".to_string(),
            args: json!({ "thread_id": "19c4ca084d0de349" }),
            result: Some(json!({
                "persisted": true,
                "output_ref": { "id": "artifact-456" }
            })),
            success: true,
            error: None,
            duration_ms: 6,
            iteration: 1,
            timestamp_ms: 3_000,
            requested_output_mode: Some("persist".to_string()),
            resolved_output_mode: Some("persist".to_string()),
            forced_persist: Some(false),
            forced_reason: None,
        };

        let summary = format_tool_execution_batch_summary_line(&exec);
        assert!(summary.contains("ExecutionId: exec-3"));
        assert!(summary.contains("OutputRef: artifact-456"));
        assert!(summary.contains("Error: none"));
    }

    #[test]
    fn validate_tool_execution_preflight_rejects_unknown_tool_output_id() {
        let missing_id = format!("missing-{}", Uuid::new_v4());
        let args = json!({
            "id": missing_id,
            "paths": ["$.message.title"]
        });

        let err = validate_tool_execution_preflight("tool_outputs.extract", &args)
            .expect_err("expected unknown tool_outputs id to be rejected");
        assert!(err.contains("Invalid tool_outputs id"));
        assert!(err.contains("ExecutionId/OutputRef.id"));
    }

    #[test]
    fn validate_tool_execution_preflight_allows_hydrated_tool_output_id() {
        let args = json!({
            "paths": ["$.message.title"]
        });

        let result = validate_tool_execution_preflight("tool_outputs.extract", &args);
        assert!(result.is_ok(), "missing id should be hydrated later");
    }

    #[test]
    fn controller_prompt_includes_no_id_invention_rule() {
        assert!(
            CONTROLLER_PROMPT_BASE.contains(
                "If output is persisted, do not invent IDs or values; call tool_outputs.extract to obtain exact values."
            ),
            "controller prompt must include persisted output rule"
        );
    }

    // ---- Phase 3: Schema hardening regression tests ----

    #[test]
    fn controller_schema_survives_anthropic_sanitizer_with_known_diff() {
        let original = controller_output_format();
        let mut sanitized = original.clone();
        if let Some(schema) = sanitized.get_mut("schema") {
            crate::llm::strip_anthropic_unsupported_schema_keywords(schema);
        }
        // Known differences after sanitization:
        // 1. thinking.additionalProperties: true → false (Anthropic forces false on all objects)
        // Verify the sanitizer does not remove any fields or add unexpected ones.
        let orig_schema = original.get("schema").unwrap();
        let san_schema = sanitized.get("schema").unwrap();

        // Root-level keys are preserved
        let orig_keys: Vec<&str> = orig_schema
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        let san_keys: Vec<&str> = san_schema
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(orig_keys, san_keys);

        // thinking.additionalProperties differs as expected
        let thinking_orig = orig_schema
            .get("properties")
            .unwrap()
            .get("thinking")
            .unwrap();
        let thinking_san = san_schema
            .get("properties")
            .unwrap()
            .get("thinking")
            .unwrap();
        assert_eq!(
            thinking_orig
                .get("additionalProperties")
                .and_then(|v| v.as_bool()),
            Some(true),
            "original thinking has additionalProperties: true"
        );
        assert_eq!(
            thinking_san
                .get("additionalProperties")
                .and_then(|v| v.as_bool()),
            Some(false),
            "sanitized thinking has additionalProperties: false (Anthropic requirement)"
        );
    }

    #[test]
    fn controller_schema_passes_anthropic_validation() {
        let format = controller_output_format();
        let mut sanitized = format.clone();
        if let Some(schema) = sanitized.get_mut("schema") {
            crate::llm::strip_anthropic_unsupported_schema_keywords(schema);
        }
        crate::llm::validate_anthropic_output_format(Some(&sanitized))
            .expect("controller schema should pass Anthropic validation");
    }

    #[test]
    fn controller_schema_has_no_conditional_keywords_at_any_depth() {
        let format = controller_output_format();
        let schema_str = serde_json::to_string(&format).unwrap();
        let forbidden = [
            "\"oneOf\"",
            "\"allOf\"",
            "\"anyOf\"",
            "\"if\"",
            "\"then\"",
            "\"else\"",
        ];
        for keyword in &forbidden {
            assert!(
                !schema_str.contains(keyword),
                "controller schema must not contain {keyword} at any depth"
            );
        }
    }

    #[test]
    fn controller_schema_root_has_additional_properties_false() {
        let format = controller_output_format();
        let schema = format.get("schema").expect("schema root");
        assert_eq!(
            schema.get("additionalProperties").and_then(|v| v.as_bool()),
            Some(false),
            "root schema must have additionalProperties: false"
        );
    }

    #[test]
    fn controller_schema_args_field_is_string_type() {
        let format = controller_output_format();
        let schema = format.get("schema").expect("schema root");
        let args = schema
            .get("properties")
            .and_then(|p| p.get("args"))
            .expect("args field");
        assert_eq!(
            args.get("type").and_then(|v| v.as_str()),
            Some("string"),
            "args should be typed as string (tool args parsed by normalize_tool_args)"
        );
    }

    #[test]
    fn controller_schema_confidence_has_no_numeric_bounds() {
        let format = controller_output_format();
        let schema = format.get("schema").expect("schema root");
        let confidence = schema
            .get("properties")
            .and_then(|p| p.get("thinking"))
            .and_then(|t| t.get("properties"))
            .and_then(|p| p.get("confidence"))
            .expect("confidence field");
        assert!(confidence.get("minimum").is_none());
        assert!(confidence.get("maximum").is_none());
        assert!(confidence.get("exclusiveMinimum").is_none());
        assert!(confidence.get("exclusiveMaximum").is_none());
    }
}
