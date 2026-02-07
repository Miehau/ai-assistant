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
    base_system_prompt: Option<String>,
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
                    step,
                } => {
                    self.ensure_plan(user_message)?;
                    match self.execute_step(call_llm, step)? {
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
}

fn default_resume_target() -> ResumeTarget {
    ResumeTarget::Reflecting
}

fn parse_controller_action(value: &Value) -> Result<ControllerAction, String> {
    match serde_json::from_value::<ControllerAction>(value.clone()) {
        Ok(action) => Ok(action),
        Err(err) => {
            let action = value.get("action").and_then(|val| val.as_str());
            if action == Some("next_step") {
                let thinking = parse_thinking(value)?;
                let step = if value.get("step").is_none() {
                    parse_next_step_payload_without_message_fallback(value)
                        .or_else(|| synthesize_step_from_thinking(&thinking))
                } else {
                    parse_next_step_payload(value)
                        .or_else(|| synthesize_step_from_thinking(&thinking))
                };

                if let Some(step) = step {
                    return Ok(ControllerAction::NextStep { thinking, step });
                }

                if let Some(question) = non_empty_string_field(value, &["question"]) {
                    return Ok(ControllerAction::AskUser {
                        question,
                        context: non_empty_string_field(value, &["context"]),
                        resume_to: parse_resume_target(value.get("resume_to")),
                    });
                }

                if let Some(message) = non_empty_string_field(value, &["message", "response"]) {
                    return Ok(ControllerAction::Complete { message });
                }
            }

            if action == Some("respond") {
                if let Some(step_value) = value.get("step") {
                    if let Some(step) =
                        parse_controller_step_payload(step_value.clone(), Some("respond"))
                    {
                        let thinking = parse_thinking(value)?;
                        return Ok(ControllerAction::NextStep { thinking, step });
                    }
                }

                if let Some(message) = non_empty_string_field(value, &["message"]) {
                    return Ok(ControllerAction::Complete { message });
                }

                if let Some(message) = non_empty_string_field(value, &["response"]) {
                    return Ok(ControllerAction::Complete { message });
                }
            }

            if action == Some("ask_user") {
                if let Some(step_value) = value.get("step") {
                    if let Some(step) =
                        parse_controller_step_payload(step_value.clone(), Some("ask_user"))
                    {
                        let thinking = parse_thinking(value)?;
                        return Ok(ControllerAction::NextStep { thinking, step });
                    }
                }

                if let Some(question) = non_empty_string_field(value, &["question"]) {
                    return Ok(ControllerAction::AskUser {
                        question,
                        context: non_empty_string_field(value, &["context"]),
                        resume_to: parse_resume_target(value.get("resume_to")),
                    });
                }
            }

            Err(format!("Invalid controller output: {err}"))
        }
    }
}

fn parse_next_step_payload(value: &Value) -> Option<ControllerStep> {
    parse_next_step_payload_inner(value, true)
}

fn parse_next_step_payload_without_message_fallback(value: &Value) -> Option<ControllerStep> {
    parse_next_step_payload_inner(value, false)
}

fn parse_next_step_payload_inner(
    value: &Value,
    include_message_fields_in_top_level_fallback: bool,
) -> Option<ControllerStep> {
    for key in ["step", "next_step", "next_action"] {
        if let Some(step_value) = value.get(key) {
            if let Some(step) = parse_controller_step_payload(step_value.clone(), None) {
                return Some(step);
            }
        }
    }

    let Value::Object(root) = value else {
        return None;
    };

    let mut step = serde_json::Map::new();
    for key in ["type", "description", "tool", "args"] {
        if let Some(field) = root.get(key) {
            if matches!(key, "type" | "description" | "tool") && is_blank_string_value(field) {
                continue;
            }
            step.insert(key.to_string(), field.clone());
        }
    }

    for key in ["tool_name", "name", "tool_args", "arguments", "tool_input"] {
        if let Some(field) = root.get(key) {
            step.insert(key.to_string(), field.clone());
        }
    }

    if include_message_fields_in_top_level_fallback {
        for key in ["message", "question", "context", "resume_to"] {
            if let Some(field) = root.get(key) {
                if matches!(key, "message" | "question" | "context") && is_blank_string_value(field)
                {
                    continue;
                }
                step.insert(key.to_string(), field.clone());
            }
        }
    }

    if step.is_empty() {
        return None;
    }

    parse_controller_step_payload(Value::Object(step), None)
}

fn synthesize_step_from_thinking(thinking: &Value) -> Option<ControllerStep> {
    if let Some(tool_step) = synthesize_tool_step_from_thinking(thinking) {
        return Some(tool_step);
    }

    let description = thinking
        .get("task")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Think through the next step")
        .to_string();

    serde_json::from_value::<ControllerStep>(json!({
        "type": "think",
        "description": description
    }))
    .ok()
}

fn synthesize_tool_step_from_thinking(thinking: &Value) -> Option<ControllerStep> {
    let tool = thinking
        .get("tool")
        .or_else(|| thinking.get("tool_name"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| is_valid_tool_name(value))
        .map(|value| value.to_string())
        .or_else(|| {
            thinking
                .get("task")
                .and_then(|value| value.as_str())
                .and_then(extract_tool_name_from_text)
        })
        .or_else(|| {
            thinking
                .get("decisions")
                .and_then(|value| value.as_array())
                .and_then(|decisions| {
                    decisions
                        .iter()
                        .find_map(|entry| entry.as_str().and_then(extract_tool_name_from_text))
                })
        })?;

    let description = thinking
        .get("task")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Call the selected tool")
        .to_string();

    let args = thinking
        .get("args")
        .or_else(|| thinking.get("tool_args"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    serde_json::from_value::<ControllerStep>(json!({
        "type": "tool",
        "description": description,
        "tool": tool,
        "args": args
    }))
    .ok()
}

fn extract_tool_name_from_text(text: &str) -> Option<String> {
    for raw_token in text.split_whitespace() {
        let token = raw_token
            .trim_matches(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'));
        if is_valid_tool_name(token) {
            return Some(token.to_string());
        }
    }
    None
}

fn is_valid_tool_name(value: &str) -> bool {
    if value.is_empty() || !value.contains('.') || value.starts_with('.') || value.ends_with('.') {
        return false;
    }

    if matches!(value.to_ascii_lowercase().as_str(), "e.g" | "i.e") {
        return false;
    }

    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
    {
        return false;
    }

    let segments = value.split('.').collect::<Vec<_>>();
    if segments.len() < 2 || segments.iter().any(|segment| segment.trim().is_empty()) {
        return false;
    }

    let action = segments[1].to_ascii_lowercase();
    if segments[1].contains('_') {
        return true;
    }

    [
        "list", "get", "read", "search", "open", "create", "update", "delete", "send", "write",
        "fetch", "find", "run", "execute", "call",
    ]
    .iter()
    .any(|prefix| action.starts_with(prefix))
}

fn parse_controller_step_payload(
    value: Value,
    forced_type: Option<&str>,
) -> Option<ControllerStep> {
    let mut step = value;
    let Value::Object(map) = &mut step else {
        return None;
    };

    for key in [
        "type",
        "description",
        "tool",
        "message",
        "question",
        "context",
    ] {
        if map.get(key).is_some_and(is_blank_string_value) {
            map.remove(key);
        }
    }

    if map.get("type").is_none() {
        if let Some(step_type) = forced_type.or_else(|| infer_step_type(map)) {
            map.insert("type".to_string(), Value::String(step_type.to_string()));
        }
    }

    if map.get("tool").is_none() {
        if let Some(tool_name) = map
            .get("tool_name")
            .or_else(|| map.get("name"))
            .and_then(|value| value.as_str())
        {
            map.insert("tool".to_string(), Value::String(tool_name.to_string()));
        }
    }

    if map.get("args").is_none() {
        if let Some(args) = map
            .get("tool_args")
            .or_else(|| map.get("arguments"))
            .or_else(|| map.get("tool_input"))
        {
            map.insert("args".to_string(), args.clone());
        }
    }

    if map.get("message").is_none() {
        if let Some(message) = map
            .get("response")
            .or_else(|| map.get("content"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert("message".to_string(), Value::String(message.to_string()));
        }
    }

    if map.get("description").is_none() {
        if let Some(step_type) = map.get("type").and_then(|value| value.as_str()) {
            map.insert(
                "description".to_string(),
                Value::String(default_step_description(step_type).to_string()),
            );
        }
    }

    serde_json::from_value::<ControllerStep>(step).ok()
}

fn infer_step_type(map: &serde_json::Map<String, Value>) -> Option<&'static str> {
    if map.get("tool").is_some_and(is_non_empty_string_value)
        || map.get("tool_name").is_some_and(is_non_empty_string_value)
    {
        return Some("tool");
    }
    if map.get("question").is_some_and(is_non_empty_string_value) {
        return Some("ask_user");
    }
    if map.get("message").is_some_and(is_non_empty_string_value) {
        return Some("respond");
    }
    if map
        .get("description")
        .is_some_and(is_non_empty_string_value)
    {
        return Some("think");
    }
    None
}

fn default_step_description(step_type: &str) -> &'static str {
    match step_type {
        "tool" => "Call the selected tool",
        "respond" => "Respond to the user",
        "think" => "Think through the next step",
        "ask_user" => "Ask the user for clarification",
        _ => "Continue with the next step",
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

fn is_non_empty_string_value(value: &Value) -> bool {
    value
        .as_str()
        .map(|text| !text.trim().is_empty())
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
        "oneOf": [
            {
                "properties": { "action": { "const": "next_step" } },
                "required": ["action", "step", "thinking"]
            },
            {
                "properties": { "action": { "const": "complete" } },
                "required": ["action", "message"]
            },
            {
                "properties": { "action": { "const": "guardrail_stop" } },
                "required": ["action", "reason"]
            },
            {
                "properties": { "action": { "const": "ask_user" } },
                "required": ["action", "question"]
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

fn hydrate_tool_args_for_execution(
    tool_name: &str,
    args: Value,
    conversation_id: &str,
    last_step_result: Option<&StepResult>,
    history: &[StepResult],
) -> Value {
    if tool_name != "tool_outputs.read" {
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
            ControllerAction::NextStep { step, .. } => match step {
                ControllerStep::Tool { tool, args, .. } => {
                    assert_eq!(tool, "weather");
                    assert_eq!(args, json!({ "location": "Austin, TX" }));
                }
                other => panic!("expected tool step, got {other:?}"),
            },
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_next_step_step_without_type_or_description() {
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
            ControllerAction::NextStep { step, .. } => match step {
                ControllerStep::Tool {
                    description,
                    tool,
                    args,
                } => {
                    assert_eq!(description, "Call the selected tool");
                    assert_eq!(tool, "weather");
                    assert_eq!(args, json!({ "location": "Austin, TX" }));
                }
                other => panic!("expected tool step, got {other:?}"),
            },
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_next_step_with_only_thinking_task() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Inspect project files before deciding" }
        });

        let action =
            parse_controller_action(&payload).expect("next_step without step should synthesize");
        match action {
            ControllerAction::NextStep { step, .. } => match step {
                ControllerStep::Think { description } => {
                    assert_eq!(description, "Inspect project files before deciding");
                }
                other => panic!("expected think step, got {other:?}"),
            },
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_ignores_blank_question_on_next_step_fallback() {
        let payload = json!({
            "action": "next_step",
            "thinking": { "task": "Inspect project files before deciding" },
            "question": "",
            "context": ""
        });

        let action = parse_controller_action(&payload).expect("payload should parse");
        match action {
            ControllerAction::NextStep { step, .. } => match step {
                ControllerStep::Think { description } => {
                    assert_eq!(description, "Inspect project files before deciding");
                }
                other => panic!("expected think step, got {other:?}"),
            },
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_does_not_treat_next_step_progress_message_as_complete() {
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
            ControllerAction::NextStep { step, .. } => match step {
                ControllerStep::Think { description } => {
                    assert_eq!(
                        description,
                        "Examine email threads to find meeting information from yesterday"
                    );
                }
                other => panic!("expected think step, got {other:?}"),
            },
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_synthesizes_tool_from_thinking_decisions() {
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

        let action = parse_controller_action(&payload).expect("payload should parse");
        match action {
            ControllerAction::NextStep { step, .. } => match step {
                ControllerStep::Tool {
                    description,
                    tool,
                    args,
                } => {
                    assert_eq!(
                        description,
                        "Check the user's last 20 emails to find meetings from yesterday"
                    );
                    assert_eq!(tool, "gmail.list_threads");
                    assert_eq!(args, json!({}));
                }
                other => panic!("expected tool step, got {other:?}"),
            },
            other => panic!("expected next_step action, got {other:?}"),
        }
    }

    #[test]
    fn controller_output_schema_uses_one_of_for_action_requirements() {
        let schema = controller_output_format();
        let root = schema.get("schema").expect("schema root");

        assert!(root.get("allOf").is_none());
        assert!(root.get("oneOf").is_some());
    }

    #[test]
    fn hydrate_tool_outputs_read_args_uses_last_step_output_ref() {
        let last_result = StepResult {
            step_id: "step-1".to_string(),
            success: true,
            output: Some(json!({
                "message": "Tool output stored in app data. Use tool_outputs.read to retrieve.",
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
}
