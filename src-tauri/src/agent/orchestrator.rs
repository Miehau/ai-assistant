use crate::agent::controller_parsing::{
    controller_output_format, default_step_description, extract_json, infer_step_type_flat,
    normalize_tool_args, parse_controller_action, parse_output_mode_hint, ControllerAction,
    ControllerToolCallSpec, OutputDeliveryResolution, OutputModeHint,
};
use crate::agent::output_delivery::build_tool_output_delivery;
use crate::agent::prompts::CONTROLLER_PROMPT_BASE;
use crate::agent::text_utils::{
    compact_history_messages_with_limits, summarize_goal, summarize_tool_args,
};
use crate::agent::tool_arg_hydration::{
    hydrate_download_path_for_execution, hydrate_tool_args_for_execution,
    validate_tool_execution_preflight,
};
use crate::agent::tool_execution::{
    build_batch_step_result, build_tool_batch_result_summary, build_tool_execution_record,
    format_tool_execution_batch_summary_line, format_tool_execution_summary_block,
};
use crate::db::{
    AgentConfig, AgentSession, AgentSessionOperations, MessageToolExecutionInput, PhaseKind, Plan,
    PlanStep, ResumeTarget, StepAction, StepResult, StepStatus, ToolBatchToolCall,
};
#[cfg(debug_assertions)]
use crate::db::{MessageAgentThinkingInput, MessageOperations};
use crate::events::{
    AgentEvent, EventBus, EVENT_AGENT_COMPANION_TEXT, EVENT_AGENT_COMPLETED,
    EVENT_AGENT_PHASE_CHANGED, EVENT_AGENT_PLAN_ADJUSTED, EVENT_AGENT_PLAN_CREATED,
    EVENT_AGENT_STEP_COMPLETED, EVENT_AGENT_STEP_PROPOSED, EVENT_AGENT_STEP_STARTED,
    EVENT_TOOL_EXECUTION_APPROVED, EVENT_TOOL_EXECUTION_COMPLETED, EVENT_TOOL_EXECUTION_DENIED,
    EVENT_TOOL_EXECUTION_PROPOSED, EVENT_TOOL_EXECUTION_STARTED,
};
use crate::llm::{LlmMessage, StreamResult};
use crate::tools::{
    get_conversation_tool_approval_override, get_tool_approval_override,
    load_conversation_tool_approval_overrides, load_tool_approval_overrides, ApprovalStore,
    PendingToolApprovalInput, ToolApprovalDecision, ToolDefinition, ToolExecutionContext,
    ToolMetadata, ToolRegistry,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use uuid::Uuid;

const CONTROLLER_HISTORY_MAX_CHARS: usize = 128_000;
const CONTROLLER_HISTORY_STABLE_PREFIX_MESSAGES: usize = 8;
const CONTROLLER_HISTORY_RECENT_TAIL_MESSAGES: usize = 40;
const PARALLEL_BATCH_FALLBACK_TIMEOUT_MS: u64 = 120_000;
const TOOL_DEPENDENCY_NOTE: &str = "NOTE: If you need IDs or fields from this tool's output for other tool calls, run this tool first (single tool step) and use its output in a later step. Do not combine dependent calls in the same tool_batch.";
const AGENT_SPAWN_TOOL: &str = "agent.spawn";
const GMAIL_LIST_THREADS_TOOL: &str = "gmail.list_threads";
const MAX_LLM_RETRIES: u32 = 3;

fn is_transient_llm_error(error: &str) -> bool {
    error.contains("429")
        || error.contains("503")
        || error.contains("529")
        || error.contains("rate_limit")
        || error.contains("overloaded")
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
    provider: Option<String>,
    model: Option<String>,
    is_sub_agent: bool,
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
            parent_session_id: None,
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
            provider: None,
            model: None,
            is_sub_agent: false,
        })
    }

    pub fn set_provider_info(&mut self, provider: String, model: String) {
        self.provider = Some(provider);
        self.model = Some(model);
    }

    pub fn set_parent_session_id(&mut self, parent_session_id: String) {
        self.session.parent_session_id = Some(parent_session_id);
        self.is_sub_agent = true;
        let _ = self.db.update_agent_session_parent(
            &self.session.id,
            self.session.parent_session_id.as_deref(),
        );
    }

    pub fn session_id(&self) -> &str {
        &self.session.id
    }

    fn build_tool_execution_context(&self) -> ToolExecutionContext {
        ToolExecutionContext {
            cancel_flag: Some(self.cancel_flag.clone()),
            provider: self.provider.clone(),
            model: self.model.clone(),
            conversation_id: Some(self.session.conversation_id.clone()),
            session_id: Some(self.session.id.clone()),
            is_sub_agent: self.is_sub_agent,
        }
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

            let decision = {
                let mut llm_retries = 0u32;
                loop {
                    match self.call_controller(call_llm) {
                        Ok(action) => break action,
                        Err(error)
                            if is_transient_llm_error(&error)
                                && llm_retries < MAX_LLM_RETRIES =>
                        {
                            llm_retries += 1;
                            let backoff_secs = 2u64.pow(llm_retries);
                            log::warn!(
                                "[agent] transient LLM error (attempt {}/{}), retrying in {}s: {}",
                                llm_retries,
                                MAX_LLM_RETRIES,
                                backoff_secs,
                                error
                            );
                            std::thread::sleep(Duration::from_secs(backoff_secs));
                            continue;
                        }
                        Err(error) => return Err(error),
                    }
                }
            };
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
                        .unwrap_or("tool");
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

        // Normalize args once to avoid repeated cloning + normalization
        let normalized_args = normalize_tool_args(args);

        let action = match effective_type {
            "tool" => StepAction::ToolCall {
                tool: tool.clone().unwrap_or_default(),
                args: normalized_args.clone(),
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
                        preview_fn(normalized_args.clone(), ToolExecutionContext::default()).ok()
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
                resume_to.unwrap_or_else(|| crate::agent::controller_parsing::default_resume_target()),
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
                    normalized_args,
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
        let args = match hydrate_download_path_for_execution(tool_name, args.clone(), &self.db) {
            Ok(args) => args,
            Err(err) => {
                return Ok(self.build_preflight_failed_step_result(
                    step_id,
                    tool_name,
                    args,
                    iteration,
                    err,
                ));
            }
        };

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
        let args = match self.tool_registry.coerce_and_validate_args(&tool.metadata, args) {
            Ok(coerced) => coerced,
            Err(err) => {
                return Ok(self.build_preflight_failed_step_result(
                    step_id,
                    tool_name,
                    json!({}),
                    iteration,
                    err.message,
                ));
            }
        };
        if let Err(err) = validate_tool_execution_preflight(tool_name, &args) {
            return Ok(self.build_preflight_failed_step_result(
                step_id, tool_name, args, iteration, err,
            ));
        }

        let execution_id = Uuid::new_v4().to_string();
        let requires_approval =
            self.resolve_requires_approval(tool_name, tool.metadata.requires_approval);

        if requires_approval {
            let preview = match tool.preview.as_ref() {
                Some(preview_fn) => Some(
                    preview_fn(args.clone(), ToolExecutionContext::default()).map_err(|err| err.message)?,
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
                tool_name, execution_id, approval_id, iteration,
                self.session.id, self.session.conversation_id, self.assistant_message_id
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
                        tool_name, execution_id, approval_id, iteration,
                        self.session.id, self.session.conversation_id, self.assistant_message_id
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
                        tool_name, execution_id, approval_id, iteration,
                        self.session.id, self.session.conversation_id, self.assistant_message_id
                    );
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_TOOL_EXECUTION_DENIED,
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
                    let exec_record = build_tool_execution_record(
                        execution_id.clone(), tool_name.to_string(), args.clone(),
                        None, false, Some(denied_error.clone()), 0,
                        iteration as usize, timestamp_ms,
                        Some(requested_output_mode), None,
                    );
                    self.push_pending_tool_execution(
                        execution_id, tool_name, args, json!(null), false, 0,
                        timestamp_ms, Some(denied_error.clone()), iteration as i64,
                    );
                    return Ok(StepResult {
                        step_id: step_id.to_string(),
                        success: false,
                        output: None,
                        error: Some(denied_error),
                        tool_executions: vec![exec_record],
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
            tool_name, execution_id, requires_approval, self.tool_calls_in_current_step,
            self.session.id, self.session.conversation_id, self.assistant_message_id, args_summary
        );
        self.publish_tool_execution_started(
            &execution_id, tool_name, &args, requires_approval,
            self.tool_calls_in_current_step,
        );

        let start = Instant::now();
        let result = self.execute_tool_with_timeout(tool, args.clone());
        let duration_ms = start.elapsed().as_millis() as i64;
        let completed_at = Utc::now();
        let timestamp_ms = completed_at.timestamp_millis();

        let (success, output, error, output_delivery, artifact_persist_warning) = match result {
            Ok(output_value) => {
                let delivery_result = build_tool_output_delivery(
                    &execution_id,
                    tool_name,
                    &self.session.conversation_id,
                    &self.assistant_message_id,
                    &args,
                    output_value,
                    requested_output_mode,
                    &tool.metadata.result_mode,
                );
                (
                    delivery_result.success,
                    delivery_result.output,
                    delivery_result.error,
                    Some(delivery_result.delivery),
                    delivery_result.artifact_persist_warning,
                )
            }
            Err(error_message) => (
                false,
                Some(json!({ "message": error_message, "success": false })),
                Some(error_message),
                None,
                None,
            ),
        };

        self.publish_tool_execution_completed(
            &execution_id, tool_name, success, output.as_ref(), error.as_deref(),
            duration_ms, self.tool_calls_in_current_step,
            artifact_persist_warning.as_deref(), timestamp_ms,
        );
        if success {
            log::info!(
                "[tool] execution completed: tool={} execution_id={} duration_ms={} success=true session_id={} conversation_id={} message_id={}",
                tool_name, execution_id, duration_ms,
                self.session.id, self.session.conversation_id, self.assistant_message_id
            );
        } else {
            log::warn!(
                "[tool] execution failed: tool={} execution_id={} duration_ms={} error={} session_id={} conversation_id={} message_id={}",
                tool_name, execution_id, duration_ms,
                error.as_deref().unwrap_or("Tool execution failed"),
                self.session.id, self.session.conversation_id, self.assistant_message_id
            );
        }

        let exec_record = build_tool_execution_record(
            execution_id.clone(), tool_name.to_string(), args.clone(),
            output.clone(), success, error.clone(), duration_ms,
            self.tool_calls_in_current_step as usize, timestamp_ms,
            Some(requested_output_mode), output_delivery.as_ref(),
        );

        self.push_pending_tool_execution(
            execution_id, tool_name, args,
            output.clone().unwrap_or_else(|| json!(null)),
            success, duration_ms, timestamp_ms, error.clone(),
            self.tool_calls_in_current_step as i64,
        );

        Ok(StepResult {
            step_id: step_id.to_string(),
            success,
            output,
            error,
            tool_executions: vec![exec_record],
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
        let (calls, dependency_dropped) = isolate_gmail_list_threads_batch(calls);
        if dependency_dropped > 0 {
            log::warn!(
                "[tool_batch] gmail.list_threads must run alone; dropped {} other call(s)",
                dependency_dropped
            );
        }
        let mut dropped_calls = dependency_dropped;

        if calls.is_empty() {
            let error = "tool_batch requires at least one tool call".to_string();
            return Ok(StepResult {
                step_id: step_id.to_string(),
                success: false,
                output: Some(json!({ "success": false, "message": error.clone() })),
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
                requested_calls, remaining_capacity
            );
            return Ok(StepResult {
                step_id: step_id.to_string(),
                success: false,
                output: Some(json!({
                    "success": false, "message": error.clone(),
                    "requested_calls": requested_calls,
                    "remaining_tool_calls": remaining_capacity
                })),
                error: Some(error),
                tool_executions: Vec::new(),
                duration_ms: 0,
                completed_at,
            });
        }
        let (calls, capacity_dropped) =
            clamp_tool_batch_calls_to_remaining_capacity(calls, remaining_capacity);
        dropped_calls += capacity_dropped;
        if capacity_dropped > 0 {
            log::warn!(
                "[tool_batch] requested {} calls but only {} are allowed in one step; executing first {} and dropping {}",
                requested_calls, remaining_capacity, calls.len(), capacity_dropped
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
                step_id, calls, requested_calls, dropped_calls,
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
            let args = match hydrate_download_path_for_execution(&tool_name, args.clone(), &self.db)
            {
                Ok(args) => args,
                Err(err) => {
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
            };
            let tool = match self.tool_registry.get(&tool_name) {
                Some(tool) => tool.clone(),
                None => {
                    let failed = self.build_preflight_failed_step_result(
                        step_id, &tool_name, args, iteration,
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
            let args = match self.tool_registry.coerce_and_validate_args(&tool.metadata, args) {
                Ok(coerced) => coerced,
                Err(err) => {
                    let failed = self.build_preflight_failed_step_result(
                        step_id, &tool_name, json!({}), iteration, err.message,
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
                tool_name, execution_id, iteration,
                self.session.id, self.session.conversation_id, self.assistant_message_id, args_summary
            );
            self.publish_tool_execution_started(
                &execution_id, &tool_name, &args, false, iteration,
            );

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
            runnable_calls.len(), timeout_ms
        );
        let (tx, rx) = mpsc::channel::<Result<ParallelToolRunResult, ()>>();
        let total_threads = runnable_calls.len();
        for call in runnable_calls {
            let cancel_flag = self.cancel_flag.clone();
            let call_for_panic = PanicFallbackInput::from(&call);
            let ctx = self.build_tool_execution_context();
            let tx = tx.clone();
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute_parallel_tool_call(call, timeout_ms, cancel_flag, ctx)
                }))
                .unwrap_or_else(|_| ParallelToolRunResult::from_panic(call_for_panic));
                let _ = tx.send(Ok(result));
            });
        }
        drop(tx); // Drop sender so rx terminates when all threads finish

        let mut run_results = Vec::new();
        let mut received = 0usize;
        loop {
            if self.is_cancelled() {
                log::info!(
                    "[tool_batch] cancellation detected during parallel join, stopping wait ({}/{} received)",
                    received, total_threads
                );
                break;
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(result)) => {
                    run_results.push(result);
                    received += 1;
                    if received >= total_threads {
                        break;
                    }
                }
                Ok(Err(_)) => {
                    first_error.get_or_insert_with(|| {
                        "Parallel tool execution worker panicked".to_string()
                    });
                    received += 1;
                    if received >= total_threads {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Continue polling — check cancel flag on next iteration
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // All senders dropped — remaining threads panicked without sending
                    if received < total_threads {
                        first_error.get_or_insert_with(|| {
                            "Parallel tool execution worker panicked".to_string()
                        });
                    }
                    break;
                }
            }
        }
        run_results.sort_by_key(|result| result.iteration);

        for result in run_results {
            self.tool_calls_in_current_step = self.tool_calls_in_current_step.max(result.iteration);
            self.publish_tool_execution_completed(
                &result.execution_id, &result.tool_name, result.success,
                result.output.as_ref(), result.error.as_deref(),
                result.duration_ms, result.iteration,
                result.artifact_persist_warning.as_deref(), result.timestamp_ms,
            );
            if result.success {
                successful_calls += 1;
            } else if first_error.is_none() {
                first_error = result.error.clone();
            }

            let execution = build_tool_execution_record(
                result.execution_id.clone(), result.tool_name.clone(), result.args.clone(),
                result.output.clone(), result.success, result.error.clone(),
                result.duration_ms, result.iteration as usize, result.timestamp_ms,
                Some(result.requested_output_mode), result.output_delivery.as_ref(),
            );
            results_summary.push(build_tool_batch_result_summary(&execution));
            aggregated_tool_executions.push(execution);

            self.push_pending_tool_execution(
                result.execution_id, &result.tool_name, result.args,
                result.output.unwrap_or_else(|| json!(null)),
                result.success, result.duration_ms, result.timestamp_ms,
                result.error, result.iteration as i64,
            );
        }

        Ok(build_batch_step_result(
            step_id, started, "parallel", requested_calls, dropped_calls,
            successful_calls, results_summary, aggregated_tool_executions, first_error,
        ))
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
            if self.is_cancelled() {
                log::info!("[tool_batch] cancellation detected between sequential tool executions");
                break;
            }
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

        Ok(build_batch_step_result(
            step_id, started, "sequential", requested_calls, dropped_calls,
            successful_calls, results_summary, aggregated_tool_executions, first_error,
        ))
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
        let output = json!({ "message": &error_message, "success": false });
        let args_summary = summarize_tool_args(&args, 500);

        log::warn!(
            "[tool] preflight failed: tool={} execution_id={} iteration={} session_id={} conversation_id={} message_id={} error={} args={}",
            tool_name, execution_id, iteration,
            self.session.id, self.session.conversation_id, self.assistant_message_id,
            error_message, args_summary
        );
        self.record_trace(
            "tool_preflight_error",
            error_message.clone(),
            iteration as i64,
            Some(json!({ "tool": tool_name, "args_summary": args_summary })),
        );
        self.publish_tool_execution_completed(
            &execution_id, tool_name, false, None, Some(&error_message),
            0, iteration, None, timestamp_ms,
        );

        self.push_pending_tool_execution(
            execution_id.clone(), tool_name, args.clone(), output.clone(),
            false, 0, timestamp_ms, Some(error_message.clone()), iteration as i64,
        );

        StepResult {
            step_id: step_id.to_string(),
            success: false,
            output: Some(output.clone()),
            error: Some(error_message.clone()),
            tool_executions: vec![build_tool_execution_record(
                execution_id, tool_name.to_string(), args,
                Some(output), false, Some(error_message), 0,
                iteration as usize, timestamp_ms, None, None,
            )],
            duration_ms: 0,
            completed_at,
        }
    }

    fn execute_tool_with_timeout(
        &self,
        tool: &ToolDefinition,
        args: Value,
    ) -> Result<Value, String> {
        let ctx = self.build_tool_execution_context();
        let timeout_ms = if tool.metadata.name == AGENT_SPAWN_TOOL {
            0
        } else {
            self.session.config.tool_execution_timeout_ms
        };
        execute_tool_handler_with_timeout(
            self.cancel_flag.clone(),
            timeout_ms,
            tool.handler.clone(),
            args,
            ctx,
        )
    }

    fn get_compacted_history_messages(&self) -> Vec<LlmMessage> {
        compact_history_messages_with_limits(
            &self.messages,
            CONTROLLER_HISTORY_MAX_CHARS,
            CONTROLLER_HISTORY_STABLE_PREFIX_MESSAGES,
            CONTROLLER_HISTORY_RECENT_TAIL_MESSAGES,
        )
    }

    fn build_controller_messages(&self, tool_list: &str) -> Vec<LlmMessage> {
        let mut messages = Vec::new();

        messages.push(LlmMessage {
            role: "system".to_string(),
            content: json!(CONTROLLER_PROMPT_BASE),
        });

        messages.push(LlmMessage {
            role: "system".to_string(),
            content: json!(format!("AVAILABLE TOOLS (JSON):\n{}", tool_list)),
        });

        messages.push(LlmMessage {
            role: "system".to_string(),
            content: json!(self.render_controller_limits()),
        });

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
        let (tool_list, tools_count) = {
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
            annotate_dependency_tool_descriptions(&mut tools);
            let count = tools.len();
            (
                serde_json::to_string(&tools).unwrap_or_else(|_| "[]".to_string()),
                count,
            )
        };

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
                "available_tools_count": tools_count
            })),
        );
        let (response, companion_text) =
            self.call_llm_json(call_llm, &messages, Some(controller_output_format()))?;
        let response_payload = serde_json::to_string_pretty(&response)
            .unwrap_or_else(|_| response.to_string());
        self.record_trace(
            "controller_output",
            response_payload,
            trace_iteration,
            None,
        );

        // Surface companion text (text blocks emitted alongside tool calls) to the
        // frontend and append to conversation context so future turns have it.
        if let Some(text) = &companion_text {
            self.event_bus.publish(AgentEvent::new_with_timestamp(
                EVENT_AGENT_COMPANION_TEXT,
                json!({
                    "session_id": self.session.id,
                    "text": text,
                }),
                Utc::now().timestamp_millis(),
            ));
            self.messages.push(LlmMessage {
                role: "assistant".to_string(),
                content: json!(text),
            });
        }

        parse_controller_action(&response)
    }

    fn call_llm_json<F>(
        &mut self,
        call_llm: &mut F,
        messages: &[LlmMessage],
        output_format: Option<Value>,
    ) -> Result<(Value, Option<String>), String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        let response = (call_llm)(messages, None, output_format)?;
        let companion_text = response.companion_text;
        let json_text = extract_json(&response.content);
        let value = serde_json::from_str(&json_text)
            .map_err(|err| format!("Invalid JSON: {err}"))?;
        Ok((value, companion_text))
    }

    fn append_tool_result_message(&mut self) {
        let Some(last_result) = self.session.step_results.last() else {
            return;
        };
        if last_result.tool_executions.is_empty() {
            return;
        }

        // Collect content blocks declared by tools (e.g. image blocks from files.read).
        // Tools that produce rich content place a `content_blocks` array in their result.
        // The orchestrator lifts these into the LLM message without inspecting their format.
        let rich_content_blocks: Vec<Value> = last_result
            .tool_executions
            .iter()
            .filter(|exec| exec.success)
            .filter_map(|exec| {
                exec.result
                    .as_ref()?
                    .get("content_blocks")?
                    .as_array()
                    .cloned()
            })
            .flatten()
            .collect();

        // Strip content_blocks (and raw base64 data) from the text summary so it
        // doesn't pollute the context with duplicated binary data.
        let formatted_execs: Vec<_> = if rich_content_blocks.is_empty() {
            last_result.tool_executions.iter().cloned().collect()
        } else {
            last_result
                .tool_executions
                .iter()
                .map(|exec| {
                    let has_blocks = exec
                        .result
                        .as_ref()
                        .and_then(|r| r.get("content_blocks"))
                        .is_some();
                    if has_blocks {
                        let mut cloned = exec.clone();
                        if let Some(obj) = cloned.result.as_mut().and_then(|r| r.as_object_mut()) {
                            obj.remove("content_blocks");
                            // Redact raw base64 content if present (e.g. files.read)
                            if obj.get("media_type")
                                .and_then(|v| v.as_str())
                                .map(|t| t.starts_with("image/"))
                                .unwrap_or(false)
                            {
                                obj.insert(
                                    "content".to_string(),
                                    json!("[binary data attached as content block]"),
                                );
                            }
                        }
                        cloned
                    } else {
                        exec.clone()
                    }
                })
                .collect()
        };

        let blocks: Vec<String> = if formatted_execs.len() > 1 {
            formatted_execs
                .iter()
                .map(format_tool_execution_batch_summary_line)
                .collect()
        } else {
            formatted_execs
                .iter()
                .map(format_tool_execution_summary_block)
                .collect()
        };

        if blocks.is_empty() {
            return;
        }

        let summary_text = format!("[Tool executions]\n{}", blocks.join("\n"));
        if rich_content_blocks.is_empty() {
            self.messages.push(LlmMessage {
                role: "user".to_string(),
                content: json!(summary_text),
            });
        } else {
            // Build a mixed content message: text summary + tool-declared content blocks.
            // parse_content_blocks in llm/mod.rs will lift these into typed ContentBlocks,
            // and provider formatters (e.g. Anthropic) will map them to native format.
            let mut content = vec![json!({"type": "text", "text": summary_text})];
            content.extend(rich_content_blocks);
            self.messages.push(LlmMessage {
                role: "user".to_string(),
                content: json!(content),
            });
        }
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

    // --- Consolidated event publishing helpers ---

    fn publish_tool_execution_started(
        &self,
        execution_id: &str,
        tool_name: &str,
        args: &Value,
        requires_approval: bool,
        iteration: u32,
    ) {
        let timestamp_ms = Utc::now().timestamp_millis();
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_TOOL_EXECUTION_STARTED,
            json!({
                "execution_id": execution_id,
                "tool_name": tool_name,
                "args": args,
                "requires_approval": requires_approval,
                "iteration": iteration,
                "conversation_id": self.session.conversation_id,
                "message_id": self.assistant_message_id,
                "session_id": self.session.id,
                "parent_session_id": self.session.parent_session_id,
                "is_sub_agent": self.is_sub_agent,
                "timestamp_ms": timestamp_ms,
            }),
            timestamp_ms,
        ));
    }

    fn publish_tool_execution_completed(
        &self,
        execution_id: &str,
        tool_name: &str,
        success: bool,
        result: Option<&Value>,
        error: Option<&str>,
        duration_ms: i64,
        iteration: u32,
        artifact_persist_warning: Option<&str>,
        timestamp_ms: i64,
    ) {
        let mut payload = if success {
            json!({
                "execution_id": execution_id,
                "tool_name": tool_name,
                "result": result.unwrap_or(&json!(null)),
                "success": true,
                "duration_ms": duration_ms,
                "iteration": iteration,
                "conversation_id": self.session.conversation_id,
                "message_id": self.assistant_message_id,
                "session_id": self.session.id,
                "parent_session_id": self.session.parent_session_id,
                "is_sub_agent": self.is_sub_agent,
                "timestamp_ms": timestamp_ms,
            })
        } else {
            json!({
                "execution_id": execution_id,
                "tool_name": tool_name,
                "success": false,
                "error": error.unwrap_or("Tool execution failed"),
                "duration_ms": duration_ms,
                "iteration": iteration,
                "conversation_id": self.session.conversation_id,
                "message_id": self.assistant_message_id,
                "session_id": self.session.id,
                "parent_session_id": self.session.parent_session_id,
                "is_sub_agent": self.is_sub_agent,
                "timestamp_ms": timestamp_ms,
            })
        };
        if let Some(warning) = artifact_persist_warning {
            payload["artifact_persist_warning"] = Value::String(warning.to_string());
        }
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_TOOL_EXECUTION_COMPLETED,
            payload,
            timestamp_ms,
        ));
    }

    fn push_pending_tool_execution(
        &mut self,
        execution_id: impl Into<String>,
        tool_name: impl Into<String>,
        parameters: Value,
        result: Value,
        success: bool,
        duration_ms: i64,
        timestamp_ms: i64,
        error: Option<String>,
        iteration_number: i64,
    ) {
        self.pending_tool_executions
            .push(MessageToolExecutionInput {
                id: execution_id.into(),
                message_id: self.assistant_message_id.clone(),
                tool_name: tool_name.into(),
                parameters,
                result,
                success,
                duration_ms,
                timestamp_ms,
                error,
                iteration_number,
                session_id: Some(self.session.id.clone()),
                parent_session_id: self.session.parent_session_id.clone(),
                is_sub_agent: self.is_sub_agent,
            });
    }

    pub fn take_tool_executions(&mut self) -> Vec<MessageToolExecutionInput> {
        std::mem::take(&mut self.pending_tool_executions)
    }

    pub fn requested_user_input(&self) -> bool {
        self.requested_user_input
    }
}

#[cfg(test)]
impl DynamicController {
    pub(super) fn test_session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
    }

    pub(super) fn test_session(&self) -> &AgentSession {
        &self.session
    }

    pub(super) fn test_cancel_flag(&self) -> &Arc<AtomicBool> {
        &self.cancel_flag
    }
}

// --- Parallel tool execution ---

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

/// Lightweight struct for panic fallback — avoids cloning the full ToolDefinition.
struct PanicFallbackInput {
    iteration: u32,
    execution_id: String,
    tool_name: String,
    args: Value,
    requested_output_mode: OutputModeHint,
}

impl From<&ParallelToolCallInput> for PanicFallbackInput {
    fn from(call: &ParallelToolCallInput) -> Self {
        Self {
            iteration: call.iteration,
            execution_id: call.execution_id.clone(),
            tool_name: call.tool_name.clone(),
            args: call.args.clone(),
            requested_output_mode: call.requested_output_mode,
        }
    }
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
    fn from_panic(call: PanicFallbackInput) -> Self {
        Self {
            iteration: call.iteration,
            execution_id: call.execution_id,
            tool_name: call.tool_name,
            args: call.args,
            requested_output_mode: call.requested_output_mode,
            output_delivery: None,
            success: false,
            output: Some(json!({ "message": "Tool execution panicked", "success": false })),
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
    ctx: ToolExecutionContext,
) -> ParallelToolRunResult {
    let start = Instant::now();

    let execution_result = execute_tool_handler_with_timeout(
        cancel_flag,
        timeout_ms,
        call.tool.handler.clone(),
        call.args.clone(),
        ctx,
    );

    let (success, output, error, output_delivery, artifact_persist_warning) = match execution_result
    {
        Ok(output_value) => {
            let delivery_result = build_tool_output_delivery(
                &call.execution_id,
                &call.tool_name,
                &call.conversation_id,
                &call.message_id,
                &call.args,
                output_value,
                call.requested_output_mode,
                &call.tool.metadata.result_mode,
            );
            (
                delivery_result.success,
                delivery_result.output,
                delivery_result.error,
                Some(delivery_result.delivery),
                delivery_result.artifact_persist_warning,
            )
        }
        Err(error_message) => (
            false,
            Some(json!({ "message": error_message, "success": false })),
            Some(error_message),
            None,
            None,
        ),
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
    ctx: ToolExecutionContext,
) -> Result<Value, String> {
    if timeout_ms == 0 {
        return (handler)(args, ctx).map_err(|err| err.message);
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send((handler)(args, ctx));
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

// --- Utility free functions ---

enum StepExecutionOutcome {
    Continue,
    Complete(String),
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

fn annotate_dependency_tool_descriptions(tools: &mut [ToolMetadata]) {
    for tool in tools {
        if !tool_name_likely_produces_ids(&tool.name) {
            continue;
        }
        if tool.description.contains(TOOL_DEPENDENCY_NOTE) {
            continue;
        }
        let trimmed = tool.description.trim_end();
        if trimmed.is_empty() {
            tool.description = TOOL_DEPENDENCY_NOTE.to_string();
        } else {
            tool.description = format!("{trimmed} {TOOL_DEPENDENCY_NOTE}");
        }
    }
}

fn tool_name_likely_produces_ids(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    let patterns = [
        ".list", ".search", ".query", ".find", ".lookup", ".discover",
    ];
    patterns.iter().any(|pattern| lower.contains(pattern))
}

fn isolate_gmail_list_threads_batch(
    calls: Vec<ControllerToolCallSpec>,
) -> (Vec<ControllerToolCallSpec>, usize) {
    let has_list_threads = calls
        .iter()
        .any(|call| call.tool.trim() == GMAIL_LIST_THREADS_TOOL);
    if !has_list_threads || calls.len() <= 1 {
        return (calls, 0);
    }

    let mut kept = Vec::new();
    let mut dropped = 0usize;
    for call in calls {
        if kept.is_empty() && call.tool.trim() == GMAIL_LIST_THREADS_TOOL {
            kept.push(call);
        } else {
            dropped += 1;
        }
    }

    (kept, dropped)
}

// --- Inline tests ---

#[cfg(test)]
mod tests {
    use super::*;

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
}
