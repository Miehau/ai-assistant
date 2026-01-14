use serde::Serialize;
use serde_json::Value;
use std::sync::{mpsc, Arc, Mutex};

pub const EVENT_MESSAGE_SAVED: &str = "message.saved";
pub const EVENT_CONVERSATION_UPDATED: &str = "conversation.updated";
pub const EVENT_CONVERSATION_DELETED: &str = "conversation.deleted";
pub const EVENT_MESSAGE_USAGE_SAVED: &str = "message.usage.saved";
pub const EVENT_USAGE_UPDATED: &str = "usage.updated";
pub const EVENT_ASSISTANT_STREAM_STARTED: &str = "assistant.stream.started";
pub const EVENT_ASSISTANT_STREAM_COMPLETED: &str = "assistant.stream.completed";
pub const EVENT_TOOL_EXECUTION_STARTED: &str = "tool.execution.started";
pub const EVENT_TOOL_EXECUTION_COMPLETED: &str = "tool.execution.completed";
pub const EVENT_TOOL_EXECUTION_PROPOSED: &str = "tool.execution.proposed";
pub const EVENT_TOOL_EXECUTION_APPROVED: &str = "tool.execution.approved";
pub const EVENT_TOOL_EXECUTION_DENIED: &str = "tool.execution.denied";
pub const EVENT_AGENT_PHASE_CHANGED: &str = "agent.phase.changed";
pub const EVENT_AGENT_TRIAGE_COMPLETED: &str = "agent.triage.completed";
pub const EVENT_AGENT_PLAN_CREATED: &str = "agent.plan.created";
pub const EVENT_AGENT_PLAN_ADJUSTED: &str = "agent.plan.adjusted";
pub const EVENT_AGENT_STEP_PROPOSED: &str = "agent.step.proposed";
pub const EVENT_AGENT_STEP_APPROVED: &str = "agent.step.approved";
pub const EVENT_AGENT_STEP_STARTED: &str = "agent.step.started";
pub const EVENT_AGENT_STEP_COMPLETED: &str = "agent.step.completed";
pub const EVENT_AGENT_REFLECTION_COMPLETED: &str = "agent.reflection.completed";
pub const EVENT_AGENT_NEEDS_HUMAN_INPUT: &str = "agent.needs_human_input";
pub const EVENT_AGENT_COMPLETED: &str = "agent.completed";

#[derive(Clone, Debug, Serialize)]
pub struct AgentEvent {
    pub event_type: String,
    pub payload: Value,
    pub timestamp_ms: i64,
}

impl AgentEvent {
    pub fn new_with_timestamp(
        event_type: impl Into<String>,
        payload: Value,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            payload,
            timestamp_ms,
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<mpsc::Sender<AgentEvent>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn subscribe(&self) -> mpsc::Receiver<AgentEvent> {
        let (tx, rx) = mpsc::channel();
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.push(tx);
        rx
    }

    pub fn publish(&self, event: AgentEvent) {
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|sender| sender.send(event.clone()).is_ok());
    }
}
