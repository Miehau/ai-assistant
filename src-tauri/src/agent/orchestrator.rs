use crate::agent::prompts::{CLARIFY_PROMPT, PLAN_PROMPT, REFLECT_PROMPT, TRIAGE_PROMPT};
use crate::agent::risk::RiskClassifier;
use crate::agent::stores::{HumanInputStore, StepApprovalDecision, StepApprovalStore};
use crate::db::{
    AgentConfig,
    AgentSession,
    AgentSessionOperations,
    ApprovalDecision,
    GatheredInfo,
    InfoSource,
    MessageToolExecutionInput,
    PhaseKind,
    Plan,
    PlanStep,
    ResumeTarget,
    StepAction,
    StepApproval,
    StepResult,
    StepStatus,
    ToolExecutionRecord,
};
use crate::events::{
    AgentEvent,
    EventBus,
    EVENT_AGENT_COMPLETED,
    EVENT_AGENT_NEEDS_HUMAN_INPUT,
    EVENT_AGENT_PHASE_CHANGED,
    EVENT_AGENT_PLAN_ADJUSTED,
    EVENT_AGENT_PLAN_CREATED,
    EVENT_AGENT_REFLECTION_COMPLETED,
    EVENT_AGENT_STEP_APPROVED,
    EVENT_AGENT_STEP_COMPLETED,
    EVENT_AGENT_STEP_PROPOSED,
    EVENT_AGENT_STEP_STARTED,
    EVENT_AGENT_TRIAGE_COMPLETED,
};
use crate::llm::{LlmMessage, StreamResult};
use crate::tools::{ToolExecutionContext, ToolRegistry};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Instant;
use uuid::Uuid;

pub struct PhaseOrchestrator {
    db: crate::db::Db,
    event_bus: EventBus,
    tool_registry: ToolRegistry,
    approvals: StepApprovalStore,
    human_input: HumanInputStore,
    risk: RiskClassifier,
    session: AgentSession,
    messages: Vec<LlmMessage>,
    base_system_prompt: Option<String>,
    assistant_message_id: String,
    pending_tool_executions: Vec<MessageToolExecutionInput>,
}

impl PhaseOrchestrator {
    pub fn new(
        db: crate::db::Db,
        event_bus: EventBus,
        tool_registry: ToolRegistry,
        approvals: StepApprovalStore,
        human_input: HumanInputStore,
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
            phase: PhaseKind::Triage,
            plan: None,
            gathered_info: Vec::new(),
            step_results: Vec::new(),
            config: AgentConfig::default(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };

        AgentSessionOperations::save_agent_session(&db, &session)
            .map_err(|e| e.to_string())?;

        Ok(Self {
            db,
            event_bus,
            tool_registry,
            approvals,
            human_input,
            risk: RiskClassifier::new(),
            session,
            messages,
            base_system_prompt,
            assistant_message_id,
            pending_tool_executions: Vec::new(),
        })
    }

    pub fn run<F>(&mut self, user_message: &str, call_llm: &mut F) -> Result<String, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let mut turns = 0u32;
        self.publish_phase_change(self.session.phase.clone());
        loop {
            if turns >= self.session.config.max_total_llm_turns {
                return Err("Exceeded maximum LLM turns".to_string());
            }
            turns += 1;

            match self.session.phase.clone() {
                PhaseKind::Triage => {
                    let triage = self.call_triage(call_llm, user_message)?;
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_AGENT_TRIAGE_COMPLETED,
                        json!({
                            "session_id": self.session.id,
                            "decision": triage.decision,
                            "reasoning": triage.reasoning,
                        }),
                        Utc::now().timestamp_millis(),
                    ));

                    match triage.decision.as_str() {
                        "direct_response" => {
                            let response = triage.response.unwrap_or_else(|| "".to_string());
                            self.transition_to(PhaseKind::Complete {
                                final_response: response.clone(),
                            })?;
                            continue;
                        }
                        "needs_clarification" => {
                            self.transition_to(PhaseKind::Clarifying {
                                attempts: 0,
                                pending_questions: Vec::new(),
                            })?;
                        }
                        "ready_to_plan" => {
                            self.transition_to(PhaseKind::Planning { revision: 0 })?;
                        }
                        _ => {
                            self.transition_to(PhaseKind::Clarifying {
                                attempts: 0,
                                pending_questions: Vec::new(),
                            })?;
                        }
                    }
                }
                PhaseKind::Clarifying {
                    attempts,
                    pending_questions: _,
                } => {
                    if attempts >= self.session.config.max_clarify_iters {
                        self.transition_to(PhaseKind::Planning { revision: 0 })?;
                        continue;
                    }

                    let clarify = self.call_clarify(call_llm, user_message)?;
                    if clarify.needs_user_input && !clarify.questions.is_empty() {
                        let question = clarify.questions.join("\n");
                        let answer = self.ask_human(question.clone(), ResumeTarget::Clarifying)?;
                        self.session.gathered_info.push(GatheredInfo {
                            question,
                            answer,
                            source: InfoSource::User,
                            gathered_at: Utc::now(),
                        });
                        self.transition_to(PhaseKind::Clarifying {
                            attempts: attempts + 1,
                            pending_questions: Vec::new(),
                        })?;
                    } else if !clarify.questions.is_empty() {
                        for assumption in clarify.assumptions {
                            self.session.gathered_info.push(GatheredInfo {
                                question: assumption.clone(),
                                answer: assumption,
                                source: InfoSource::Assumption,
                                gathered_at: Utc::now(),
                            });
                        }
                        self.transition_to(PhaseKind::Planning { revision: 0 })?;
                    } else {
                        self.transition_to(PhaseKind::Planning { revision: 0 })?;
                    }
                }
                PhaseKind::Planning { revision } => {
                    let plan = self.call_plan(call_llm, user_message, revision)?;
                    self.session.plan = Some(plan.clone());
                    AgentSessionOperations::save_agent_plan(&self.db, &self.session.id, &plan)
                        .map_err(|e| e.to_string())?;
                    AgentSessionOperations::save_plan_steps(&self.db, &plan.id, &plan.steps)
                        .map_err(|e| e.to_string())?;
                    let event_type = if revision == 0 {
                        EVENT_AGENT_PLAN_CREATED
                    } else {
                        EVENT_AGENT_PLAN_ADJUSTED
                    };
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        event_type,
                        json!({
                            "session_id": self.session.id,
                            "plan": plan,
                        }),
                        Utc::now().timestamp_millis(),
                    ));
                    self.transition_to(PhaseKind::ProposingStep { step_index: 0 })?;
                }
                PhaseKind::ProposingStep { .. } => {
                    let next_index = self.select_next_step_index();
                    if next_index.is_none() {
                        self.transition_to(PhaseKind::Complete {
                            final_response: "Task completed.".to_string(),
                        })?;
                        continue;
                    }
                    let step_index = next_index.unwrap();
                    let (step_id, step_snapshot, needs_approval, risk_label, approval_request, preview, plan_revision) = {
                        let plan = self.session.plan.as_mut().ok_or("Missing plan")?;
                        let step = plan.steps.get_mut(step_index).ok_or("Invalid step index")?;
                        step.status = StepStatus::Proposed;
                        AgentSessionOperations::update_plan_step_status(&self.db, &step.id, StepStatus::Proposed)
                            .map_err(|e| e.to_string())?;

                        let (needs_approval, risk_label, approval_request) = match &step.action {
                            StepAction::ToolCall { tool, .. } => {
                                let risk = self.risk.classify(tool);
                                let needs = self.risk.requires_approval(risk);
                                let request = if needs {
                                    let (approval_id, approval_rx) = self.approvals.create_request();
                                    Some((approval_id, approval_rx))
                                } else {
                                    None
                                };
                                (needs, format!("{:?}", risk), request)
                            }
                            _ => (false, "None".to_string(), None),
                        };

                        let preview = match &step.action {
                            StepAction::ToolCall { tool, args } => {
                                self.tool_registry.get(tool).and_then(|tool_def| {
                                    tool_def.preview.as_ref().and_then(|preview| {
                                        preview(args.clone(), ToolExecutionContext).ok()
                                    })
                                })
                            }
                            _ => None,
                        };

                        (
                            step.id.clone(),
                            step.clone(),
                            needs_approval,
                            risk_label,
                            approval_request,
                            preview,
                            plan.revision_count,
                        )
                    };

                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_AGENT_STEP_PROPOSED,
                        json!({
                            "session_id": self.session.id,
                            "step": step_snapshot,
                            "risk": risk_label,
                            "approval_id": approval_request.as_ref().map(|(id, _)| id.clone()),
                            "preview": preview,
                        }),
                        Utc::now().timestamp_millis(),
                    ));

                    if !needs_approval {
                        let approval = StepApproval {
                            decision: ApprovalDecision::AutoApproved {
                                reason: "Risk auto-approval".to_string(),
                            },
                            feedback: None,
                            decided_at: Utc::now(),
                        };
                        if let Some(step) = self.session.plan.as_mut().and_then(|plan| plan.steps.iter_mut().find(|s| s.id == step_id)) {
                            step.status = StepStatus::Approved;
                            step.approval = Some(approval.clone());
                        }
                        AgentSessionOperations::update_plan_step_status(&self.db, &step_id, StepStatus::Approved)
                            .map_err(|e| e.to_string())?;
                        AgentSessionOperations::save_step_approval(&self.db, &step_id, &approval)
                            .map_err(|e| e.to_string())?;
                        self.event_bus.publish(AgentEvent::new_with_timestamp(
                            EVENT_AGENT_STEP_APPROVED,
                            json!({
                                "session_id": self.session.id,
                                "step_id": step_id,
                                "decision": "auto_approved",
                            }),
                            Utc::now().timestamp_millis(),
                        ));
                        self.transition_to(PhaseKind::Executing {
                            step_id: step_id.clone(),
                            tool_iteration: 0,
                        })?;
                        continue;
                    }

                    let (approval_id, approval_rx) =
                        approval_request.ok_or("Missing approval request")?;
                    let decision = approval_rx
                        .recv_timeout(std::time::Duration::from_millis(
                            self.session.config.approval_timeout_ms,
                        ))
                        .map_err(|_| "Step approval timeout".to_string())?;

                    match decision {
                        StepApprovalDecision::Approved => {
                            let approval = StepApproval {
                                decision: ApprovalDecision::Approved,
                                feedback: None,
                                decided_at: Utc::now(),
                            };
                            if let Some(step) = self.session.plan.as_mut().and_then(|plan| plan.steps.iter_mut().find(|s| s.id == step_id)) {
                                step.status = StepStatus::Approved;
                                step.approval = Some(approval.clone());
                            }
                            AgentSessionOperations::update_plan_step_status(&self.db, &step_id, StepStatus::Approved)
                                .map_err(|e| e.to_string())?;
                            AgentSessionOperations::save_step_approval(&self.db, &step_id, &approval)
                                .map_err(|e| e.to_string())?;
                            self.event_bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_AGENT_STEP_APPROVED,
                                json!({
                                    "session_id": self.session.id,
                                    "step_id": step_id,
                                    "decision": "approved",
                                    "approval_id": approval_id,
                                }),
                                Utc::now().timestamp_millis(),
                            ));
                            self.transition_to(PhaseKind::Executing {
                                step_id: step_id.clone(),
                                tool_iteration: 0,
                            })?;
                        }
                        StepApprovalDecision::Skipped => {
                            let approval = StepApproval {
                                decision: ApprovalDecision::Skipped,
                                feedback: None,
                                decided_at: Utc::now(),
                            };
                            if let Some(step) = self.session.plan.as_mut().and_then(|plan| plan.steps.iter_mut().find(|s| s.id == step_id)) {
                                step.status = StepStatus::Skipped;
                                step.approval = Some(approval.clone());
                            }
                            AgentSessionOperations::update_plan_step_status(&self.db, &step_id, StepStatus::Skipped)
                                .map_err(|e| e.to_string())?;
                            AgentSessionOperations::save_step_approval(&self.db, &step_id, &approval)
                                .map_err(|e| e.to_string())?;
                            self.event_bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_AGENT_STEP_APPROVED,
                                json!({
                                    "session_id": self.session.id,
                                    "step_id": step_id,
                                    "decision": "skipped",
                                    "approval_id": approval_id,
                                }),
                                Utc::now().timestamp_millis(),
                            ));
                            self.transition_to(PhaseKind::ProposingStep { step_index })?;
                        }
                        StepApprovalDecision::Modified { feedback } => {
                            let approval = StepApproval {
                                decision: ApprovalDecision::Modified,
                                feedback: feedback.clone(),
                                decided_at: Utc::now(),
                            };
                            if let Some(step) = self.session.plan.as_mut().and_then(|plan| plan.steps.iter_mut().find(|s| s.id == step_id)) {
                                step.approval = Some(approval.clone());
                            }
                            AgentSessionOperations::save_step_approval(&self.db, &step_id, &approval)
                                .map_err(|e| e.to_string())?;
                            self.event_bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_AGENT_STEP_APPROVED,
                                json!({
                                    "session_id": self.session.id,
                                    "step_id": step_id,
                                    "decision": "modified",
                                    "approval_id": approval_id,
                                    "feedback": feedback,
                                }),
                                Utc::now().timestamp_millis(),
                            ));
                            let revision = plan_revision + 1;
                            self.transition_to(PhaseKind::Planning { revision })?;
                        }
                        StepApprovalDecision::Denied { feedback } => {
                            let approval = StepApproval {
                                decision: ApprovalDecision::Denied,
                                feedback: feedback.clone(),
                                decided_at: Utc::now(),
                            };
                            if let Some(step) = self.session.plan.as_mut().and_then(|plan| plan.steps.iter_mut().find(|s| s.id == step_id)) {
                                step.approval = Some(approval.clone());
                            }
                            AgentSessionOperations::save_step_approval(&self.db, &step_id, &approval)
                                .map_err(|e| e.to_string())?;
                            self.event_bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_AGENT_STEP_APPROVED,
                                json!({
                                    "session_id": self.session.id,
                                    "step_id": step_id,
                                    "decision": "denied",
                                    "approval_id": approval_id,
                                    "feedback": feedback,
                                }),
                                Utc::now().timestamp_millis(),
                            ));
                            let _ = self.ask_human(
                                "What would you like me to do instead?".to_string(),
                                ResumeTarget::Planning {
                                    revision: plan_revision + 1,
                                },
                            )?;
                            let revision = plan_revision + 1;
                            self.transition_to(PhaseKind::Planning { revision })?;
                        }
                    }
                }
                PhaseKind::Executing { step_id, tool_iteration } => {
                    if tool_iteration >= self.session.config.max_tool_calls_per_step {
                        return Err("Exceeded tool call limit".to_string());
                    }

                    let action = {
                        let plan = self.session.plan.as_mut().ok_or("Missing plan")?;
                        let step = plan
                            .steps
                            .iter_mut()
                            .find(|s| s.id == step_id)
                            .ok_or("Step not found")?;
                        step.status = StepStatus::Executing;
                        AgentSessionOperations::update_plan_step_status(&self.db, &step.id, StepStatus::Executing)
                            .map_err(|e| e.to_string())?;
                        step.action.clone()
                    };

                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_AGENT_STEP_STARTED,
                        json!({
                            "session_id": self.session.id,
                            "step_id": step_id,
                        }),
                        Utc::now().timestamp_millis(),
                    ));

                    let result = match action {
                        StepAction::ToolCall { tool, args } => {
                            self.execute_tool(step_id.clone(), &tool, args)?
                        }
                        StepAction::AskUser { question } => {
                            let answer = self.ask_human(question.clone(), ResumeTarget::Reflecting)?;
                            StepResult {
                                step_id: step_id.clone(),
                                success: true,
                                output: Some(json!({ "answer": answer })),
                                error: None,
                                tool_executions: Vec::new(),
                                duration_ms: 0,
                                completed_at: Utc::now(),
                            }
                        }
                        StepAction::Think { prompt } => {
                            let output = self.call_think(call_llm, &prompt)?;
                            StepResult {
                                step_id: step_id.clone(),
                                success: true,
                                output: Some(json!({ "output": output })),
                                error: None,
                                tool_executions: Vec::new(),
                                duration_ms: 0,
                                completed_at: Utc::now(),
                            }
                        }
                    };

                    if let Some(step) = self.session.plan.as_mut().and_then(|plan| plan.steps.iter_mut().find(|s| s.id == step_id)) {
                        step.result = Some(result.clone());
                        step.status = if result.success {
                            StepStatus::Completed
                        } else {
                            StepStatus::Failed
                        };
                        AgentSessionOperations::update_plan_step_status(&self.db, &step.id, step.status.clone())
                            .map_err(|e| e.to_string())?;
                    }
                    AgentSessionOperations::save_step_result(&self.db, &result)
                        .map_err(|e| e.to_string())?;
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_AGENT_STEP_COMPLETED,
                        json!({
                            "session_id": self.session.id,
                            "step_id": step_id,
                            "success": result.success,
                            "result": result.output,
                            "error": result.error,
                        }),
                        Utc::now().timestamp_millis(),
                    ));

                    self.transition_to(PhaseKind::Reflecting)?;
                }
                PhaseKind::Reflecting => {
                    let (plan_snapshot, last_step_snapshot, revision) = {
                        let plan = self.session.plan.as_ref().ok_or("Missing plan")?;
                        let last_step = plan
                            .steps
                            .iter()
                            .filter(|s| matches!(s.status, StepStatus::Completed | StepStatus::Failed))
                            .max_by(|a, b| a.sequence.cmp(&b.sequence))
                            .ok_or("No completed step to reflect on")?;
                        (plan.clone(), last_step.clone(), plan.revision_count)
                    };
                    let reflect = self.call_reflect(call_llm, &plan_snapshot, &last_step_snapshot)?;
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_AGENT_REFLECTION_COMPLETED,
                        json!({
                            "session_id": self.session.id,
                            "decision": reflect.decision,
                            "reason": reflect.reason,
                        }),
                        Utc::now().timestamp_millis(),
                    ));

                    match reflect.decision.as_str() {
                        "continue" => {
                            self.transition_to(PhaseKind::ProposingStep { step_index: 0 })?;
                        }
                        "adjust" => {
                            self.transition_to(PhaseKind::Planning { revision: revision + 1 })?;
                        }
                        "need_more_info" => {
                            self.transition_to(PhaseKind::Clarifying {
                                attempts: 0,
                                pending_questions: Vec::new(),
                            })?;
                        }
                        "done" => {
                            let summary = reflect.summary.unwrap_or_else(|| "Done.".to_string());
                            self.transition_to(PhaseKind::Complete {
                                final_response: summary.clone(),
                            })?;
                            continue;
                        }
                        "need_human_input" => {
                            let question = reflect
                                .question
                                .unwrap_or_else(|| "Can you clarify?".to_string());
                            let answer = self.ask_human(question, ResumeTarget::Reflecting)?;
                            self.session.gathered_info.push(GatheredInfo {
                                question: "Reflection follow-up".to_string(),
                                answer,
                                source: InfoSource::User,
                                gathered_at: Utc::now(),
                            });
                            self.transition_to(PhaseKind::Reflecting)?;
                        }
                        _ => {
                            self.transition_to(PhaseKind::Complete {
                                final_response: "Task completed.".to_string(),
                            })?;
                            continue;
                        }
                    }
                }
                PhaseKind::Complete { final_response } => {
                    AgentSessionOperations::update_agent_session_completed(
                        &self.db,
                        &self.session.id,
                        &final_response,
                    )
                    .map_err(|e| e.to_string())?;
                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                        EVENT_AGENT_COMPLETED,
                        json!({
                            "session_id": self.session.id,
                            "response": final_response,
                        }),
                        Utc::now().timestamp_millis(),
                    ));
                    return Ok(final_response);
                }
                PhaseKind::NeedsHumanInput { .. } => {
                    return Err("Unexpected human input phase".to_string());
                }
                PhaseKind::GuardrailStop { reason, .. } => {
                    return Err(reason);
                }
            }
        }
    }

    fn transition_to(&mut self, next: PhaseKind) -> Result<(), String> {
        let current = self.session.phase.clone();
        if !current.is_valid_transition(&next) {
            return Err("Invalid phase transition".to_string());
        }
        self.session.phase = next.clone();
        self.session.updated_at = Utc::now();
        AgentSessionOperations::update_agent_session_phase(&self.db, &self.session.id, &next)
            .map_err(|e| e.to_string())?;
        self.publish_phase_change(next);
        Ok(())
    }

    fn publish_phase_change(&self, to: PhaseKind) {
        let _ = &to;
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_PHASE_CHANGED,
            json!({
                "session_id": self.session.id,
                "phase": to,
            }),
            Utc::now().timestamp_millis(),
        ));
    }

    fn call_triage<F>(&mut self, call_llm: &mut F, user_message: &str) -> Result<TriageResponse, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let prompt = TRIAGE_PROMPT
            .replace("{user_message}", user_message)
            .replace("{recent_messages}", &self.render_history());
        let response = self.call_llm_json(call_llm, &prompt)?;
        parse_triage_response(&response)
    }

    fn call_clarify<F>(&mut self, call_llm: &mut F, user_message: &str) -> Result<ClarifyResponse, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let gathered = if self.session.gathered_info.is_empty() {
            "None".to_string()
        } else {
            self.session
                .gathered_info
                .iter()
                .map(|info| format!("- {}: {}", info.question, info.answer))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let prompt = CLARIFY_PROMPT
            .replace("{user_message}", user_message)
            .replace("{gathered_info}", &gathered);
        let response = self.call_llm_json(call_llm, &prompt)?;
        parse_clarify_response(&response)
    }

    fn call_plan<F>(
        &mut self,
        call_llm: &mut F,
        user_message: &str,
        revision: u32,
    ) -> Result<Plan, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let tool_list = serde_json::to_string(&self.tool_registry.prompt_json())
            .unwrap_or_else(|_| "[]".to_string());
        let gathered = if self.session.gathered_info.is_empty() {
            "None".to_string()
        } else {
            self.session
                .gathered_info
                .iter()
                .map(|info| format!("- {}: {}", info.question, info.answer))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let base_prompt = PLAN_PROMPT
            .replace("{user_message}", user_message)
            .replace("{gathered_info}", &gathered)
            .replace("{tool_descriptions}", &tool_list);

        let mut last_error = None;
        for attempt in 0..2 {
            let prompt = if let Some(err) = last_error.as_ref() {
                format!("{base_prompt}\n\nPrevious error: {err}\nPlease fix the plan JSON.")
            } else {
                base_prompt.clone()
            };
            let response = self.call_llm_json(call_llm, &prompt)?;
            match parse_plan_response(&response, revision, &self.tool_registry) {
                Ok(plan) => return Ok(plan),
                Err(err) => last_error = Some(err),
            }
            if attempt == 1 {
                break;
            }
        }

        Err(last_error.unwrap_or_else(|| "Failed to create plan".to_string()))
    }

    fn call_reflect<F>(
        &mut self,
        call_llm: &mut F,
        plan: &Plan,
        step: &PlanStep,
    ) -> Result<ReflectResponse, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let remaining_steps = plan
            .steps
            .iter()
            .filter(|s| matches!(s.status, StepStatus::Pending | StepStatus::Proposed | StepStatus::Approved))
            .map(|s| format!("- {}", s.description))
            .collect::<Vec<_>>()
            .join("\n");
        let step_result = step
            .result
            .as_ref()
            .map(|result| serde_json::to_string(result).unwrap_or_default())
            .unwrap_or_else(|| "No result".to_string());

        let prompt = REFLECT_PROMPT
            .replace("{plan_goal}", &plan.goal)
            .replace("{step_description}", &step.description)
            .replace("{expected_outcome}", &step.expected_outcome)
            .replace("{step_result}", &step_result)
            .replace("{remaining_steps}", &remaining_steps);
        let response = self.call_llm_json(call_llm, &prompt)?;
        parse_reflect_response(&response)
    }

    fn call_think<F>(&mut self, call_llm: &mut F, prompt: &str) -> Result<String, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let response = (call_llm)(
            &[LlmMessage {
                role: "user".to_string(),
                content: json!(prompt),
            }],
            self.base_system_prompt.as_deref(),
        )?;
        Ok(response.content)
    }

    fn call_llm_json<F>(&mut self, call_llm: &mut F, prompt: &str) -> Result<Value, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let response = (call_llm)(
            &[LlmMessage {
                role: "user".to_string(),
                content: json!(prompt),
            }],
            self.base_system_prompt.as_deref(),
        )?;
        let json_text = extract_json(&response.content);
        serde_json::from_str(&json_text).map_err(|err| format!("Invalid JSON: {err}"))
    }

    fn render_history(&self) -> String {
        self.messages
            .iter()
            .map(|message| {
                let content = value_to_string(&message.content);
                format!("{}: {}", message.role, content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn ask_human(
        &mut self,
        question: String,
        resume_to: ResumeTarget,
    ) -> Result<String, String> {
        let (request_id, rx) = self.human_input.create_request();
        let phase = PhaseKind::NeedsHumanInput {
            question: question.clone(),
            context: None,
            resume_to,
        };
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            EVENT_AGENT_NEEDS_HUMAN_INPUT,
            json!({
                "session_id": self.session.id,
                "request_id": request_id,
                "question": question,
            }),
            Utc::now().timestamp_millis(),
        ));
        self.session.phase = phase;
        AgentSessionOperations::update_agent_session_phase(&self.db, &self.session.id, &self.session.phase)
            .map_err(|e| e.to_string())?;
        self.publish_phase_change(self.session.phase.clone());
        let answer = rx
            .recv_timeout(std::time::Duration::from_millis(
                self.session.config.approval_timeout_ms,
            ))
            .map_err(|_| "Human input timeout".to_string())?;
        Ok(answer)
    }

    fn execute_tool(
        &mut self,
        step_id: String,
        tool_name: &str,
        args: Value,
    ) -> Result<StepResult, String> {
        let tool = self
            .tool_registry
            .get(tool_name)
            .ok_or_else(|| format!("Unknown tool: {tool_name}"))?;
        self.tool_registry
            .validate_args(&tool.metadata, &args)
            .map_err(|err| err.message)?;

        let execution_id = Uuid::new_v4().to_string();
        let timestamp_ms = Utc::now().timestamp_millis();
        self.event_bus.publish(AgentEvent::new_with_timestamp(
            crate::events::EVENT_TOOL_EXECUTION_STARTED,
            json!({
                "execution_id": execution_id,
                "tool_name": tool_name,
                "args": args,
                "requires_approval": false,
                "iteration": 1,
                "conversation_id": self.session.conversation_id,
                "message_id": self.assistant_message_id,
                "timestamp_ms": timestamp_ms,
            }),
            timestamp_ms,
        ));

        let start = Instant::now();
        let result = (tool.handler)(args.clone(), ToolExecutionContext);
        let duration_ms = start.elapsed().as_millis() as i64;
        let completed_at = Utc::now();
        let timestamp_ms = completed_at.timestamp_millis();

        let (success, output, error) = match result {
            Ok(output) => {
                self.event_bus.publish(AgentEvent::new_with_timestamp(
                    crate::events::EVENT_TOOL_EXECUTION_COMPLETED,
                    json!({
                        "execution_id": execution_id,
                        "tool_name": tool_name,
                        "result": output,
                        "success": true,
                        "duration_ms": duration_ms,
                        "iteration": 1,
                        "conversation_id": self.session.conversation_id,
                        "message_id": self.assistant_message_id,
                        "timestamp_ms": timestamp_ms,
                    }),
                    timestamp_ms,
                ));
                (true, Some(output), None)
            }
            Err(err) => {
                self.event_bus.publish(AgentEvent::new_with_timestamp(
                    crate::events::EVENT_TOOL_EXECUTION_COMPLETED,
                    json!({
                        "execution_id": execution_id,
                        "tool_name": tool_name,
                        "success": false,
                        "error": err.message,
                        "duration_ms": duration_ms,
                        "iteration": 1,
                        "conversation_id": self.session.conversation_id,
                        "message_id": self.assistant_message_id,
                        "timestamp_ms": timestamp_ms,
                    }),
                    timestamp_ms,
                ));
                (false, None, Some(err.message))
            }
        };

        let record = ToolExecutionRecord {
            execution_id: execution_id.clone(),
            tool_name: tool_name.to_string(),
            args: args.clone(),
            result: output.clone(),
            success,
            error: error.clone(),
            duration_ms,
            iteration: 1,
            timestamp_ms,
        };

        self.pending_tool_executions.push(MessageToolExecutionInput {
            id: execution_id,
            message_id: self.assistant_message_id.clone(),
            tool_name: tool_name.to_string(),
            parameters: args,
            result: output.clone().unwrap_or_else(|| json!(null)),
            success,
            duration_ms,
            timestamp_ms,
            error: error.clone(),
            iteration_number: 1,
        });

        Ok(StepResult {
            step_id,
            success,
            output,
            error,
            tool_executions: vec![record],
            duration_ms,
            completed_at,
        })
    }

    fn select_next_step_index(&self) -> Option<usize> {
        let plan = self.session.plan.as_ref()?;
        let mut candidates: Vec<(usize, &PlanStep)> = plan
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| {
                matches!(
                    step.status,
                    StepStatus::Executing | StepStatus::Approved | StepStatus::Proposed
                )
            })
            .collect();

        if candidates.is_empty() {
            candidates = plan
                .steps
                .iter()
                .enumerate()
                .filter(|(_, step)| step.status == StepStatus::Pending)
                .collect();
        }

        candidates.sort_by(|a, b| a.1.sequence.cmp(&b.1.sequence));
        candidates.first().map(|(idx, _)| *idx)
    }

    pub fn take_tool_executions(&mut self) -> Vec<MessageToolExecutionInput> {
        std::mem::take(&mut self.pending_tool_executions)
    }
}

#[derive(Debug, Deserialize)]
struct TriageResponse {
    decision: String,
    response: Option<String>,
    reasoning: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ClarifyResponse {
    questions: Vec<String>,
    assumptions: Vec<String>,
    needs_user_input: bool,
    reasoning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlanResponse {
    goal: String,
    assumptions: Vec<String>,
    steps: Vec<PlanStepInput>,
}

#[derive(Debug, Deserialize)]
struct PlanStepInput {
    id: Option<String>,
    description: String,
    expected_outcome: String,
    action: Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ReflectResponse {
    decision: String,
    reason: Option<String>,
    new_steps: Option<Vec<PlanStepInput>>,
    summary: Option<String>,
    question: Option<String>,
}

fn parse_triage_response(value: &Value) -> Result<TriageResponse, String> {
    serde_json::from_value(value.clone()).map_err(|err| format!("Invalid triage response: {err}"))
}

fn parse_clarify_response(value: &Value) -> Result<ClarifyResponse, String> {
    serde_json::from_value(value.clone()).map_err(|err| format!("Invalid clarify response: {err}"))
}

fn parse_plan_response(
    value: &Value,
    revision: u32,
    registry: &ToolRegistry,
) -> Result<Plan, String> {
    let parsed: PlanResponse =
        serde_json::from_value(value.clone()).map_err(|err| format!("Invalid plan response: {err}"))?;
    if parsed.steps.is_empty() {
        return Err("Plan must include at least one step".to_string());
    }
    let now = Utc::now();
    let steps = parsed
        .steps
        .into_iter()
        .enumerate()
        .map(|(sequence, step)| {
            let action = parse_step_action(step.action, registry)?;
            Ok(PlanStep {
                id: step.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                sequence,
                description: step.description,
                expected_outcome: step.expected_outcome,
                action,
                status: StepStatus::Pending,
                result: None,
                approval: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(Plan {
        id: Uuid::new_v4().to_string(),
        goal: parsed.goal,
        assumptions: parsed.assumptions,
        steps,
        revision_count: revision,
        created_at: now,
    })
}

fn parse_reflect_response(value: &Value) -> Result<ReflectResponse, String> {
    serde_json::from_value(value.clone()).map_err(|err| format!("Invalid reflect response: {err}"))
}

fn parse_step_action(value: Value, registry: &ToolRegistry) -> Result<StepAction, String> {
    if let Some(tool) = value.get("tool").and_then(|v| v.as_str()) {
        let args = value.get("args").cloned().unwrap_or_else(|| json!({}));
        if registry.get(tool).is_none() {
            return Err(format!("Unknown tool in plan: {tool}"));
        }
        return Ok(StepAction::ToolCall {
            tool: tool.to_string(),
            args,
        });
    }

    if let Some(question) = value
        .get("ask_user")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("question").and_then(|v| v.as_str()))
    {
        return Ok(StepAction::AskUser {
            question: question.to_string(),
        });
    }

    if let Some(prompt) = value
        .get("think")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("prompt").and_then(|v| v.as_str()))
    {
        return Ok(StepAction::Think {
            prompt: prompt.to_string(),
        });
    }

    Err("Unknown step action".to_string())
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
