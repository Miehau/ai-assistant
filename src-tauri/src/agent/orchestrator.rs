use crate::agent::prompts::CONTROLLER_PROMPT_BASE;
use crate::db::{
    AgentConfig, AgentSession, AgentSessionOperations, MessageToolExecutionInput, PhaseKind, Plan,
    PlanStep, ResumeTarget, StepAction, StepResult, StepStatus, ToolExecutionRecord,
};
use crate::events::{
    AgentEvent, EventBus, EVENT_AGENT_COMPLETED, EVENT_AGENT_PHASE_CHANGED,
    EVENT_AGENT_PLAN_ADJUSTED, EVENT_AGENT_PLAN_CREATED, EVENT_AGENT_STEP_COMPLETED,
    EVENT_AGENT_STEP_PROPOSED, EVENT_AGENT_STEP_STARTED, EVENT_TOOL_EXECUTION_APPROVED,
    EVENT_TOOL_EXECUTION_COMPLETED, EVENT_TOOL_EXECUTION_DENIED, EVENT_TOOL_EXECUTION_PROPOSED,
    EVENT_TOOL_EXECUTION_STARTED,
};
use crate::llm::{json_schema_output_format, LlmMessage, StreamResult};
use crate::tool_outputs::{store_tool_output, ToolOutputRecord};
use crate::tools::{
    get_conversation_tool_approval_override, get_tool_approval_override,
    load_conversation_tool_approval_overrides, load_tool_approval_overrides, ApprovalStore,
    PendingToolApprovalInput, ToolApprovalDecision, ToolExecutionContext, ToolRegistry,
    ToolResultMode,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
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

            let decision = self.call_controller(call_llm, user_message, turns)?;
            match decision {
                ControllerAction::NextStep {
                    thinking: _thinking,
                    step_type,
                    description,
                    tool,
                    args,
                    message,
                    question,
                    context,
                    resume_to,
                } => {
                    self.ensure_plan(user_message)?;
                    let effective_type = step_type
                        .as_deref()
                        .or_else(|| infer_step_type_flat(&tool, &message, &question))
                        .unwrap_or("tool"); // safe: validate() already checked
                    match self.execute_flat_step(
                        call_llm,
                        effective_type,
                        description,
                        tool,
                        args,
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
        args: Value,
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
                self.execute_tool(&step_id, &tool_name, normalize_tool_args(args))?
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
                output: Some(json!({ "question": ask_user_payload.as_ref().map(|(q, _, _)| q.clone()).unwrap_or_default() })),
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

    fn execute_tool(
        &mut self,
        step_id: &str,
        tool_name: &str,
        args: Value,
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

        let tool = self
            .tool_registry
            .get(tool_name)
            .ok_or_else(|| format!("Unknown tool: {tool_name}"))?;
        self.tool_registry
            .validate_args(&tool.metadata, &args)
            .map_err(|err| err.message)?;

        let execution_id = Uuid::new_v4().to_string();
        let mut tool_executions = Vec::new();
        let requires_approval = match get_conversation_tool_approval_override(
            &self.db,
            &self.session.conversation_id,
            tool_name,
        ) {
            Ok(Some(value)) => value,
            Ok(None) => match get_tool_approval_override(&self.db, tool_name) {
                Ok(Some(value)) => value,
                Ok(None) => tool.metadata.requires_approval,
                Err(err) => {
                    log::warn!(
                        "Failed to load global tool approval override for {}: {}",
                        tool_name,
                        err
                    );
                    tool.metadata.requires_approval
                }
            },
            Err(err) => {
                log::warn!(
                    "Failed to load conversation tool approval override for {}: {}",
                    tool_name,
                    err
                );
                tool.metadata.requires_approval
            }
        };

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
        let (success, output, error) = match result {
            Ok(output_value) => {
                let output_chars = value_char_len(&output_value);
                let persist_output =
                    should_persist_tool_output(tool_name, &tool.metadata.result_mode, output_chars);

                if !persist_output {
                    (true, Some(output_value), None)
                } else {
                    let (preview, preview_truncated) = summarize_tool_output_value(
                        &output_value,
                        PERSISTED_RESULT_PREVIEW_MAX_CHARS,
                    );
                    let record = ToolOutputRecord {
                        id: execution_id.clone(),
                        tool_name: tool_name.to_string(),
                        conversation_id: Some(self.session.conversation_id.clone()),
                        message_id: self.assistant_message_id.clone(),
                        created_at: timestamp_ms,
                        success: true,
                        parameters: args.clone(),
                        output: output_value,
                    };

                    match store_tool_output(&record) {
                        Ok(output_ref) => {
                            let metadata = compute_output_metadata(&record.output);
                            let message = json!({
                                "persisted": true,
                                "output_ref": output_ref,
                                "size_chars": output_chars as i64,
                                "preview": preview,
                                "preview_truncated": preview_truncated,
                                "metadata": metadata,
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
                        }
                        Err(err) => {
                            let error_message = format!("Failed to persist tool output: {err}");
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
                json!({
                    "execution_id": execution_id.clone(),
                    "tool_name": tool_name,
                    "result": result_for_event,
                    "success": true,
                    "duration_ms": duration_ms,
                    "iteration": self.tool_calls_in_current_step,
                    "conversation_id": self.session.conversation_id,
                    "message_id": self.assistant_message_id,
                    "timestamp_ms": timestamp_ms,
                }),
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

    fn execute_tool_with_timeout(
        &self,
        tool: &crate::tools::ToolDefinition,
        args: Value,
    ) -> Result<Value, String> {
        let timeout_ms = self.session.config.tool_execution_timeout_ms;
        if timeout_ms == 0 {
            return (tool.handler)(args, ToolExecutionContext).map_err(|err| err.message);
        }

        let handler = tool.handler.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send((handler)(args, ToolExecutionContext));
        });

        let timeout = Duration::from_millis(timeout_ms);
        let started = Instant::now();
        loop {
            if self.is_cancelled() {
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

    /// Get compacted history messages for controller context
    /// Maintains consistent message structure to preserve cache validity
    fn get_compacted_history_messages(&self) -> Vec<LlmMessage> {
        // Calculate total character count of messages
        let message_sizes: Vec<usize> = self
            .messages
            .iter()
            .map(|msg| value_to_string(&msg.content).chars().count())
            .collect();

        let total_chars: usize = message_sizes.iter().sum();

        // If under the limit, return all messages
        if total_chars <= CONTROLLER_HISTORY_MAX_CHARS {
            return self.messages.clone();
        }

        // Apply compaction: keep first N and last N messages
        let prefix_end = self
            .messages
            .len()
            .min(CONTROLLER_HISTORY_STABLE_PREFIX_MESSAGES);
        let tail_start = self
            .messages
            .len()
            .saturating_sub(CONTROLLER_HISTORY_RECENT_TAIL_MESSAGES);

        // If ranges overlap, return all messages
        if tail_start <= prefix_end {
            return self.messages.clone();
        }

        // Build compacted history with summary message
        let mut compacted = Vec::new();

        // Add stable prefix messages
        compacted.extend_from_slice(&self.messages[..prefix_end]);

        // Add summary message indicating omission
        let omitted_count = tail_start - prefix_end;
        let omitted_chars: usize = message_sizes[prefix_end..tail_start].iter().sum();

        compacted.push(LlmMessage {
            role: "system".to_string(),
            content: json!(format!(
                "[Context Summary: {} messages omitted ({} characters) to maintain conversation flow while staying within limits]",
                omitted_count, omitted_chars
            )),
        });

        // Add recent tail messages
        compacted.extend_from_slice(&self.messages[tail_start..]);

        compacted
    }

    /// Build controller messages as an array for optimal caching
    fn build_controller_messages(
        &self,
        tool_list: &str,
        user_message: &str,
        turns: u32,
    ) -> Vec<LlmMessage> {
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

        // 3. Conversation history as individual messages (CACHED prefix)
        // Apply smart compaction to keep cache stable while limiting context size
        let history_messages = self.get_compacted_history_messages();
        messages.extend(history_messages);

        // 4. Current request with dynamic context (NOT CACHED - changes every turn)
        let dynamic_context = format!(
            "USER REQUEST:\n{}\n\nLAST TOOL OUTPUT:\n{}\n\nLIMITS:\n{}",
            user_message,
            self.render_last_tool_output(),
            self.render_limits(turns)
        );

        messages.push(LlmMessage {
            role: "user".to_string(),
            content: json!(dynamic_context),
        });

        messages
    }

    fn call_controller<F>(
        &mut self,
        call_llm: &mut F,
        user_message: &str,
        turns: u32,
    ) -> Result<ControllerAction, String>
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
        let messages = self.build_controller_messages(&tool_list, user_message, turns);
        let response = self.call_llm_json(call_llm, &messages, Some(controller_output_format()))?;
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

    fn render_last_tool_output(&self) -> String {
        match self.last_step_result.as_ref() {
            Some(result) => {
                if let Some(output) = result.output.as_ref() {
                    output.to_string()
                } else if let Some(error) = result.error.as_ref() {
                    format!("error: {error}")
                } else {
                    "None".to_string()
                }
            }
            None => "None".to_string(),
        }
    }

    fn render_limits(&self, turns: u32) -> String {
        let remaining_turns = self
            .session
            .config
            .max_total_llm_turns
            .saturating_sub(turns);
        let remaining_tools = self
            .session
            .config
            .max_tool_calls_per_step
            .saturating_sub(self.tool_calls_in_current_step);
        format!(
            "Remaining turns: {}. Remaining tool calls in current step: {}.",
            remaining_turns, remaining_tools
        )
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
        #[serde(default)]
        args: Value,
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

impl ControllerAction {
    fn validate(&self) -> Result<(), String> {
        match self {
            ControllerAction::NextStep {
                step_type,
                tool,
                message,
                question,
                ..
            } => {
                let effective_type = step_type
                    .as_deref()
                    .or_else(|| infer_step_type_flat(tool, message, question));
                match effective_type {
                    Some("tool") => {
                        if tool.as_ref().map_or(true, |t| t.trim().is_empty()) {
                            return Err(
                                "next_step type=tool requires non-empty 'tool' field".into()
                            );
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
                    None => {
                        return Err(
                            "Cannot determine step type: provide 'type' or 'tool'/'message'/'question'"
                                .into(),
                        )
                    }
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
    message: &Option<String>,
    question: &Option<String>,
) -> Option<&'static str> {
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
                "enum": ["tool", "respond", "ask_user"]
            },
            "description": { "type": "string" },
            "tool": { "type": "string" },
            "args": {},
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
    if value_has_non_empty_string_field(&args, "id") {
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
        _ => {
            args = json!({
                "id": output_id,
                "conversation_id": conversation_id
            });
        }
    }

    args
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

fn summarize_tool_args(args: &Value, max_len: usize) -> String {
    let raw = serde_json::to_string(args).unwrap_or_else(|_| "<invalid-json>".to_string());
    if raw.len() <= max_len {
        return raw;
    }
    let truncated: String = raw.chars().take(max_len).collect();
    format!("{truncated}...")
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

fn extract_json(raw: &str) -> String {
    let trimmed = raw.trim();
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

fn compute_output_metadata(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let keys: Vec<&str> = map.keys().map(|k| k.as_str()).take(20).collect();
            json!({
                "root_type": "object",
                "key_count": map.len(),
                "top_level_keys": keys
            })
        }
        Value::Array(arr) => {
            json!({
                "root_type": "array",
                "array_length": arr.len()
            })
        }
        Value::String(_) => json!({ "root_type": "string" }),
        Value::Number(_) => json!({ "root_type": "number" }),
        Value::Bool(_) => json!({ "root_type": "boolean" }),
        Value::Null => json!({ "root_type": "null" }),
    }
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
        let type_enum = props
            .get("type")
            .unwrap()
            .get("enum")
            .expect("type enum");
        let values: Vec<&str> = type_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(values, vec!["tool", "respond", "ask_user"]);
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
                step_type, tool, args, ..
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
                assert_eq!(
                    message.as_deref(),
                    Some("Here is the info you requested.")
                );
            }
            other => panic!("expected next_step action, got {other:?}"),
        }
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
    fn hydrate_tool_outputs_extract_args_uses_last_step_output_ref() {
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
        assert_eq!(
            hydrated
                .get("conversation_id")
                .and_then(|value| value.as_str()),
            Some("conversation-1")
        );
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
    fn compute_output_metadata_for_object() {
        let value = json!({ "name": "test", "count": 42, "items": [1, 2, 3] });
        let meta = compute_output_metadata(&value);
        assert_eq!(meta.get("root_type").and_then(|v| v.as_str()), Some("object"));
        assert_eq!(meta.get("key_count").and_then(|v| v.as_u64()), Some(3));
        let keys = meta.get("top_level_keys").and_then(|v| v.as_array()).unwrap();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn compute_output_metadata_for_array() {
        let value = json!([1, 2, 3, 4, 5]);
        let meta = compute_output_metadata(&value);
        assert_eq!(meta.get("root_type").and_then(|v| v.as_str()), Some("array"));
        assert_eq!(meta.get("array_length").and_then(|v| v.as_u64()), Some(5));
    }

    #[test]
    fn compute_output_metadata_for_string() {
        let value = json!("hello world");
        let meta = compute_output_metadata(&value);
        assert_eq!(meta.get("root_type").and_then(|v| v.as_str()), Some("string"));
    }

    // ---- Phase 3: Schema hardening regression tests ----

    #[test]
    fn controller_schema_survives_anthropic_sanitizer_with_known_diff() {
        let original = controller_output_format();
        let mut sanitized = original.clone();
        if let Some(schema) = sanitized.get_mut("schema") {
            crate::llm::strip_anthropic_unsupported_schema_keywords(schema);
        }
        // The only expected difference is thinking.additionalProperties
        // changing from true to false (Anthropic forces false on all objects).
        // Verify the sanitizer does not remove any fields or add unexpected ones.
        let orig_schema = original.get("schema").unwrap();
        let san_schema = sanitized.get("schema").unwrap();

        // Root-level keys are preserved
        let orig_keys: Vec<&str> = orig_schema.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        let san_keys: Vec<&str> = san_schema.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        assert_eq!(orig_keys, san_keys);

        // Only the thinking object's additionalProperties should differ
        let thinking_orig = orig_schema.get("properties").unwrap().get("thinking").unwrap();
        let thinking_san = san_schema.get("properties").unwrap().get("thinking").unwrap();
        assert_eq!(
            thinking_orig.get("additionalProperties").and_then(|v| v.as_bool()),
            Some(true),
            "original thinking has additionalProperties: true"
        );
        assert_eq!(
            thinking_san.get("additionalProperties").and_then(|v| v.as_bool()),
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
        let forbidden = ["\"oneOf\"", "\"allOf\"", "\"anyOf\"", "\"if\"", "\"then\"", "\"else\""];
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
            schema
                .get("additionalProperties")
                .and_then(|v| v.as_bool()),
            Some(false),
            "root schema must have additionalProperties: false"
        );
    }

    #[test]
    fn controller_schema_args_field_accepts_any_value() {
        let format = controller_output_format();
        let schema = format.get("schema").expect("schema root");
        let args = schema
            .get("properties")
            .and_then(|p| p.get("args"))
            .expect("args field");
        // args should be an empty schema {} which accepts any value
        assert_eq!(args, &json!({}));
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
