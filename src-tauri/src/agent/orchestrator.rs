use crate::agent::prompts::CONTROLLER_PROMPT;
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

pub struct DynamicController {
    db: crate::db::Db,
    event_bus: EventBus,
    tool_registry: ToolRegistry,
    approvals: ApprovalStore,
    cancel_flag: Arc<AtomicBool>,
    session: AgentSession,
    messages: Vec<LlmMessage>,
    base_system_prompt: Option<String>,
    assistant_message_id: String,
    pending_tool_executions: Vec<MessageToolExecutionInput>,
    last_step_result: Option<StepResult>,
    tool_calls_in_current_step: u32,
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
        base_system_prompt: Option<String>,
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
            base_system_prompt,
            assistant_message_id,
            pending_tool_executions: Vec::new(),
            last_step_result: None,
            tool_calls_in_current_step: 0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_session(
        db: crate::db::Db,
        event_bus: EventBus,
        tool_registry: ToolRegistry,
        approvals: ApprovalStore,
        cancel_flag: Arc<AtomicBool>,
        session: AgentSession,
        messages: Vec<LlmMessage>,
        base_system_prompt: Option<String>,
        assistant_message_id: String,
    ) -> Self {
        Self {
            db,
            event_bus,
            tool_registry,
            approvals,
            cancel_flag,
            session,
            messages,
            base_system_prompt,
            assistant_message_id,
            pending_tool_executions: Vec::new(),
            last_step_result: None,
            tool_calls_in_current_step: 0,
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

            let decision = self.call_controller(call_llm, user_message, turns)?;
            match decision {
                ControllerAction::NextStep {
                    thinking: _thinking,
                    step,
                } => {
                    self.ensure_plan(user_message)?;
                    match self.execute_step(call_llm, step)? {
                        StepExecutionOutcome::Continue => {}
                        StepExecutionOutcome::Complete(response) => {
                            return self.finish(response);
                        }
                        StepExecutionOutcome::NeedsHumanInput(question) => {
                            return Ok(question);
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
                    self.set_phase(PhaseKind::NeedsHumanInput {
                        question: question.clone(),
                        context,
                        resume_to,
                    })?;
                    return Ok(question);
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

    fn execute_step<F>(
        &mut self,
        call_llm: &mut F,
        step: ControllerStep,
    ) -> Result<StepExecutionOutcome, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        self.tool_calls_in_current_step = 0;
        let plan = self.session.plan.as_mut().ok_or("Missing plan")?;
        let step_id = format!("step-{}", Uuid::new_v4());
        let sequence = plan.steps.len();
        let expected_outcome = "Step result recorded.".to_string();
        let action = match &step {
            ControllerStep::Tool { tool, args, .. } => StepAction::ToolCall {
                tool: tool.clone(),
                args: normalize_tool_args(args.clone()),
            },
            ControllerStep::Respond { message, .. } => StepAction::Respond {
                message: message.clone(),
            },
            ControllerStep::Think { description } => StepAction::Think {
                prompt: description.clone(),
            },
            ControllerStep::AskUser { question, .. } => StepAction::AskUser {
                question: question.clone(),
            },
        };

        let plan_step = PlanStep {
            id: step_id.clone(),
            sequence,
            description: step.description().to_string(),
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

        let preview = match &step {
            ControllerStep::Tool { tool, args, .. } => self
                .tool_registry
                .get(tool)
                .and_then(|tool_def| tool_def.preview.as_ref())
                .and_then(|preview| {
                    preview(normalize_tool_args(args.clone()), ToolExecutionContext).ok()
                }),
            _ => None,
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

        let respond_message = match &step {
            ControllerStep::Respond { message, .. } => Some(message.clone()),
            ControllerStep::AskUser { question, .. } => Some(question.clone()),
            _ => None,
        };
        let is_respond = matches!(&step, ControllerStep::Respond { .. });
        let ask_user_payload = match &step {
            ControllerStep::AskUser {
                question,
                context,
                resume_to,
                ..
            } => Some((question.clone(), context.clone(), resume_to.clone())),
            _ => None,
        };

        let result = match step {
            ControllerStep::Tool { tool, args, .. } => {
                self.execute_tool(&step_id, &tool, normalize_tool_args(args))?
            }
            ControllerStep::Respond { message, .. } => StepResult {
                step_id: step_id.clone(),
                success: true,
                output: Some(json!({ "message": message })),
                error: None,
                tool_executions: Vec::new(),
                duration_ms: 0,
                completed_at: Utc::now(),
            },
            ControllerStep::Think { description } => {
                let output = self.call_think(call_llm, &description)?;
                StepResult {
                    step_id: step_id.clone(),
                    success: true,
                    output: Some(json!({ "note": output })),
                    error: None,
                    tool_executions: Vec::new(),
                    duration_ms: 0,
                    completed_at: Utc::now(),
                }
            }
            ControllerStep::AskUser { question, .. } => StepResult {
                step_id: step_id.clone(),
                success: true,
                output: Some(json!({ "question": question })),
                error: None,
                tool_executions: Vec::new(),
                duration_ms: 0,
                completed_at: Utc::now(),
            },
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
            self.set_phase(PhaseKind::NeedsHumanInput {
                question: question.clone(),
                context,
                resume_to,
            })?;
            return Ok(StepExecutionOutcome::NeedsHumanInput(
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
                    let forced_persist_for_safety = tool.metadata.result_mode
                        == ToolResultMode::Inline
                        && output_chars > INLINE_RESULT_HARD_MAX_CHARS;
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
                            let message = json!({
                                "message": "Tool output stored in app data. Use tool_outputs.read to retrieve.",
                                "success": true,
                                "output_ref": output_ref,
                                "result_mode": "persist",
                                "requested_result_mode": &tool.metadata.result_mode,
                                "result_size_chars": output_chars as i64,
                                "forced_persist_for_safety": forced_persist_for_safety,
                                "preview": preview,
                                "preview_truncated": preview_truncated
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

    fn call_think<F>(&mut self, call_llm: &mut F, prompt: &str) -> Result<String, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        let response = (call_llm)(
            &[LlmMessage {
                role: "user".to_string(),
                content: json!(prompt),
            }],
            self.base_system_prompt.as_deref(),
            None,
        )?;
        Ok(response.content)
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
        let prompt = CONTROLLER_PROMPT
            .replace("{user_message}", user_message)
            .replace(
                "{recent_messages}",
                &self.render_history(self.messages.len()),
            )
            .replace("{state_summary}", &self.render_state_summary())
            .replace("{last_tool_output}", &self.render_last_tool_output())
            .replace("{limits}", &self.render_limits(turns))
            .replace("{tool_descriptions}", &tool_list);
        let response = self.call_llm_json(call_llm, &prompt, Some(controller_output_format()))?;
        parse_controller_action(&response)
    }

    fn call_llm_json<F>(
        &mut self,
        call_llm: &mut F,
        prompt: &str,
        output_format: Option<Value>,
    ) -> Result<Value, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        let response = (call_llm)(
            &[LlmMessage {
                role: "user".to_string(),
                content: json!(prompt),
            }],
            self.base_system_prompt.as_deref(),
            output_format,
        )?;
        let json_text = extract_json(&response.content);
        serde_json::from_str(&json_text).map_err(|err| format!("Invalid JSON: {err}"))
    }

    fn render_history(&self, limit: usize) -> String {
        let start = if self.messages.len() > limit {
            self.messages.len() - limit
        } else {
            0
        };
        self.messages
            .iter()
            .skip(start)
            .map(|message| {
                let content = value_to_string(&message.content);
                format!("{}: {}", message.role, content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_state_summary(&self) -> String {
        let mut lines = Vec::new();
        let total_steps = self
            .session
            .plan
            .as_ref()
            .map(|plan| plan.steps.len())
            .unwrap_or(0);
        lines.push(format!("Steps so far: {total_steps}"));

        if let Some(plan) = self.session.plan.as_ref() {
            for step in plan.steps.iter().rev().take(3) {
                let status = format!("{:?}", step.status);
                lines.push(format!("- {} [{}]", step.description, status));
                if let Some(result) = step.result.as_ref() {
                    if let Some(output) = result.output.as_ref() {
                        lines.push(format!("  result: {}", output));
                    } else if let Some(error) = result.error.as_ref() {
                        lines.push(format!("  error: {}", error));
                    }
                }
            }
        }

        lines.join("\n")
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

    pub fn is_waiting_for_human_input(&self) -> bool {
        matches!(self.session.phase, PhaseKind::NeedsHumanInput { .. })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ControllerAction {
    NextStep {
        thinking: Value,
        step: ControllerStep,
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

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ControllerStep {
    Tool {
        description: String,
        tool: String,
        #[serde(default)]
        args: Value,
    },
    Respond {
        description: String,
        message: String,
    },
    Think {
        description: String,
    },
    AskUser {
        description: String,
        question: String,
        #[serde(default)]
        context: Option<String>,
        #[serde(default = "default_resume_target")]
        resume_to: ResumeTarget,
    },
}

impl ControllerStep {
    fn description(&self) -> &str {
        match self {
            ControllerStep::Tool { description, .. } => description,
            ControllerStep::Respond { description, .. } => description,
            ControllerStep::Think { description } => description,
            ControllerStep::AskUser { description, .. } => description,
        }
    }
}

enum StepExecutionOutcome {
    Continue,
    Complete(String),
    NeedsHumanInput(String),
}

fn default_resume_target() -> ResumeTarget {
    ResumeTarget::Reflecting
}

fn parse_controller_action(value: &Value) -> Result<ControllerAction, String> {
    match serde_json::from_value::<ControllerAction>(value.clone()) {
        Ok(action) => Ok(action),
        Err(err) => {
            let action = value.get("action").and_then(|val| val.as_str());
            if action == Some("respond") {
                if let Some(step_value) = value.get("step") {
                    let mut step = step_value.clone();
                    if step.get("type").is_none() {
                        if let Value::Object(map) = &mut step {
                            map.insert("type".to_string(), Value::String("respond".to_string()));
                        }
                    }
                    if let Ok(step) = serde_json::from_value::<ControllerStep>(step) {
                        let thinking = parse_thinking(value)?;
                        return Ok(ControllerAction::NextStep { thinking, step });
                    }
                }

                if let Some(message) = value.get("message").and_then(|val| val.as_str()) {
                    return Ok(ControllerAction::Complete {
                        message: message.to_string(),
                    });
                }

                if let Some(message) = value.get("response").and_then(|val| val.as_str()) {
                    return Ok(ControllerAction::Complete {
                        message: message.to_string(),
                    });
                }
            }

            if action == Some("ask_user") {
                if let Some(step_value) = value.get("step") {
                    let mut step = step_value.clone();
                    if step.get("type").is_none() {
                        if let Value::Object(map) = &mut step {
                            map.insert("type".to_string(), Value::String("ask_user".to_string()));
                        }
                    }
                    if let Ok(step) = serde_json::from_value::<ControllerStep>(step) {
                        let thinking = parse_thinking(value)?;
                        return Ok(ControllerAction::NextStep { thinking, step });
                    }
                }

                if let Some(question) = value.get("question").and_then(|val| val.as_str()) {
                    return Ok(ControllerAction::AskUser {
                        question: question.to_string(),
                        context: value
                            .get("context")
                            .and_then(|val| val.as_str())
                            .map(|val| val.to_string()),
                        resume_to: parse_resume_target(value.get("resume_to")),
                    });
                }
            }

            Err(format!("Invalid controller output: {err}"))
        }
    }
}

fn parse_resume_target(value: Option<&Value>) -> ResumeTarget {
    match value.and_then(|value| value.as_str()) {
        Some("controller") => ResumeTarget::Controller,
        Some("reflecting") => ResumeTarget::Reflecting,
        _ => default_resume_target(),
    }
}

fn parse_thinking(value: &Value) -> Result<Value, String> {
    let thinking = value
        .get("thinking")
        .ok_or_else(|| "Missing required field: thinking".to_string())?;
    if !thinking.is_object() {
        return Err("Invalid field thinking: expected object".to_string());
    }
    Ok(thinking.clone())
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
            "step": {
                "type": "object",
                "required": ["type", "description"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["tool", "respond", "think", "ask_user"]
                    },
                    "description": { "type": "string" },
                    "tool": { "type": "string" },
                    "args": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "message": { "type": "string" },
                    "question": { "type": "string" },
                    "context": { "type": "string" },
                    "resume_to": {
                        "type": "string",
                        "enum": ["reflecting", "controller"]
                    }
                },
                "additionalProperties": false
            },
            "thinking": {
                "type": "object",
                "properties": {
                    "task": { "type": "string" },
                    "facts": { "type": "array", "items": { "type": "string" } },
                    "decisions": { "type": "array", "items": { "type": "string" } },
                    "risks": { "type": "array", "items": { "type": "string" } },
                    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
                },
                "additionalProperties": true
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
        "allOf": [
            {
                "if": {
                    "properties": { "action": { "const": "next_step" } }
                },
                "then": {
                    "required": ["step", "thinking"]
                }
            }
        ],
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
    if tool_name == "tool_outputs.read" {
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
