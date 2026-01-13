use crate::events::{
    AgentEvent,
    EventBus,
    EVENT_TOOL_EXECUTION_APPROVED,
    EVENT_TOOL_EXECUTION_COMPLETED,
    EVENT_TOOL_EXECUTION_DENIED,
    EVENT_TOOL_EXECUTION_PROPOSED,
    EVENT_TOOL_EXECUTION_STARTED,
};
use crate::llm::{LlmMessage, StreamResult};
use chrono::Utc;
use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

mod files;
mod search;
mod vault;

pub use files::register_file_tools;
pub use search::register_search_tool;

pub const DEFAULT_MAX_ITERATIONS: usize = 4;

#[derive(Clone, Debug, Serialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub args_schema: Value,
    pub result_schema: Value,
    pub requires_approval: bool,
}

#[derive(Clone)]
pub struct ToolDefinition {
    pub metadata: ToolMetadata,
    pub handler: Arc<ToolHandler>,
    pub preview: Option<Arc<ToolPreviewHandler>>,
}

pub type ToolHandler =
    dyn Fn(Value, ToolExecutionContext) -> Result<Value, ToolError> + Send + Sync;
pub type ToolPreviewHandler =
    dyn Fn(Value, ToolExecutionContext) -> Result<Value, ToolError> + Send + Sync;

#[derive(Clone, Debug)]
pub struct ToolExecutionContext {
    pub tool_name: String,
    pub execution_id: String,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub iteration: usize,
}

#[derive(Clone, Debug)]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, definition: ToolDefinition) -> Result<(), String> {
        let name = definition.metadata.name.clone();
        if self.tools.contains_key(&name) {
            return Err(format!("Tool already registered: {name}"));
        }
        self.tools.insert(name, definition);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    pub fn list_metadata(&self) -> Vec<ToolMetadata> {
        self.tools.values().map(|tool| tool.metadata.clone()).collect()
    }

    pub fn prompt_json(&self) -> Value {
        serde_json::to_value(self.list_metadata()).unwrap_or_else(|_| json!([]))
    }

    pub fn validate_args(&self, metadata: &ToolMetadata, args: &Value) -> Result<(), ToolError> {
        let schema = JSONSchema::compile(&metadata.args_schema)
            .map_err(|err| ToolError::new(format!("Invalid args schema: {err}")))?;
        if let Err(errors) = schema.validate(args) {
            let messages = errors.map(|e| e.to_string()).collect::<Vec<_>>().join("; ");
            return Err(ToolError::new(format!(
                "Invalid args for tool {}: {messages}",
                metadata.name
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ToolLoopContext {
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ToolLoopConfig {
    pub max_iterations: usize,
}

impl Default for ToolLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolLoopResponse {
    pub content: String,
    pub iterations: usize,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AgentAction {
    DirectResponse { content: String },
    ToolCalls { calls: Vec<ToolCall> },
}

#[derive(Clone, Debug, Deserialize)]
struct ToolCall {
    tool: String,
    #[serde(default)]
    args: Value,
}

#[derive(Clone)]
pub struct ApprovalStore {
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<ApprovalDecision>>>>,
}

#[derive(Clone, Debug)]
enum ApprovalDecision {
    Approved,
    Denied,
}

impl ApprovalStore {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn create_request(&self) -> (String, mpsc::Receiver<ApprovalDecision>) {
        let (tx, rx) = mpsc::channel();
        let approval_id = Uuid::new_v4().to_string();
        let mut pending = self.pending.lock().unwrap();
        pending.insert(approval_id.clone(), tx);
        (approval_id, rx)
    }

    pub fn resolve(&self, approval_id: &str, approved: bool) -> Result<(), String> {
        let sender = {
            let mut pending = self.pending.lock().unwrap();
            pending.remove(approval_id)
        };

        let sender = sender.ok_or_else(|| format!("Unknown approval id: {approval_id}"))?;
        let decision = if approved {
            ApprovalDecision::Approved
        } else {
            ApprovalDecision::Denied
        };
        sender
            .send(decision)
            .map_err(|_| "Failed to deliver approval decision".to_string())
    }
}

#[derive(Clone)]
pub struct ToolLoopRunner {
    registry: Arc<ToolRegistry>,
    approvals: Arc<ApprovalStore>,
    event_bus: EventBus,
    config: ToolLoopConfig,
}

impl ToolLoopRunner {
    pub fn new(
        registry: Arc<ToolRegistry>,
        approvals: Arc<ApprovalStore>,
        event_bus: EventBus,
        config: ToolLoopConfig,
    ) -> Self {
        Self {
            registry,
            approvals,
            event_bus,
            config,
        }
    }

    pub fn run<F>(
        &self,
        mut messages: Vec<LlmMessage>,
        base_system_prompt: Option<&str>,
        mut call_llm: F,
        context: ToolLoopContext,
    ) -> Result<ToolLoopResponse, String>
    where
        F: FnMut(&[LlmMessage], Option<&str>) -> Result<StreamResult, String>,
    {
        let system_prompt = self.build_system_prompt(base_system_prompt);
        let base_conversation_id = context.conversation_id.clone();
        let base_message_id = context.message_id.clone();

        for iteration in 0..self.config.max_iterations {
            let response = call_llm(&messages, Some(&system_prompt))?;
            let action = parse_agent_action(&response.content)?;

            match action {
                AgentAction::DirectResponse { content } => {
                    return Ok(ToolLoopResponse {
                        content,
                        iterations: iteration + 1,
                    });
                }
                AgentAction::ToolCalls { calls } => {
                    if calls.is_empty() {
                        return Err("No tool calls provided by model".to_string());
                    }

                    for call in calls {
                        let tool = self
                            .registry
                            .get(&call.tool)
                            .ok_or_else(|| format!("Unknown tool: {}", call.tool))?;
                        let tool_name = tool.metadata.name.clone();
                        let call_args = call.args.clone();

                        self.registry.validate_args(&tool.metadata, &call_args)?;

                        let execution_id = Uuid::new_v4().to_string();
                        let tool_context = ToolExecutionContext {
                            tool_name: tool_name.clone(),
                            execution_id: execution_id.clone(),
                            conversation_id: base_conversation_id.clone(),
                            message_id: base_message_id.clone(),
                            iteration: iteration + 1,
                        };

                        if tool.metadata.requires_approval {
                            let preview = match tool.preview.as_ref() {
                                Some(preview_fn) => Some(preview_fn(call_args.clone(), tool_context.clone())
                                    .map_err(|err| err.message)?),
                                None => None,
                            };

                            let (approval_id, approval_rx) = self.approvals.create_request();
                            let timestamp_ms = Utc::now().timestamp_millis();
                            self.event_bus.publish(AgentEvent::new_with_timestamp(
                                EVENT_TOOL_EXECUTION_PROPOSED,
                                json!({
                                    "execution_id": execution_id.clone(),
                                    "approval_id": approval_id.clone(),
                                    "tool_name": tool_name.clone(),
                                    "args": call_args.clone(),
                                    "preview": preview,
                                    "iteration": iteration + 1,
                                    "conversation_id": base_conversation_id.clone(),
                                    "message_id": base_message_id.clone(),
                                    "timestamp_ms": timestamp_ms,
                                }),
                                timestamp_ms,
                            ));

                            let decision = approval_rx
                                .recv()
                                .map_err(|_| "Approval channel closed".to_string())?;

                            let timestamp_ms = Utc::now().timestamp_millis();
                            match decision {
                                ApprovalDecision::Approved => {
                                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                                        EVENT_TOOL_EXECUTION_APPROVED,
                                        json!({
                                            "execution_id": execution_id.clone(),
                                            "approval_id": approval_id.clone(),
                                            "tool_name": tool_name.clone(),
                                            "iteration": iteration + 1,
                                            "conversation_id": base_conversation_id.clone(),
                                            "message_id": base_message_id.clone(),
                                            "timestamp_ms": timestamp_ms,
                                        }),
                                        timestamp_ms,
                                    ));
                                }
                                ApprovalDecision::Denied => {
                                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                                        EVENT_TOOL_EXECUTION_DENIED,
                                        json!({
                                            "execution_id": execution_id.clone(),
                                            "approval_id": approval_id.clone(),
                                            "tool_name": tool_name.clone(),
                                            "iteration": iteration + 1,
                                            "conversation_id": base_conversation_id.clone(),
                                            "message_id": base_message_id.clone(),
                                            "timestamp_ms": timestamp_ms,
                                        }),
                                        timestamp_ms,
                                    ));
                                    return Err(format!(
                                        "Tool execution denied: {}",
                                        tool_name
                                    ));
                                }
                            }
                        }

                        let timestamp_ms = Utc::now().timestamp_millis();
                        self.event_bus.publish(AgentEvent::new_with_timestamp(
                            EVENT_TOOL_EXECUTION_STARTED,
                            json!({
                                "execution_id": execution_id.clone(),
                                "tool_name": tool_name.clone(),
                                "args": call_args.clone(),
                                "requires_approval": tool.metadata.requires_approval,
                                "iteration": iteration + 1,
                                "conversation_id": base_conversation_id.clone(),
                                "message_id": base_message_id.clone(),
                                "timestamp_ms": timestamp_ms,
                            }),
                            timestamp_ms,
                        ));

                        let start = Instant::now();
                        let result = (tool.handler)(call_args, tool_context);
                        let duration_ms = start.elapsed().as_millis() as i64;

                        match result {
                            Ok(result) => {
                                let result_for_message = result.clone();
                                let timestamp_ms = Utc::now().timestamp_millis();
                                self.event_bus.publish(AgentEvent::new_with_timestamp(
                                    EVENT_TOOL_EXECUTION_COMPLETED,
                                    json!({
                                        "execution_id": execution_id.clone(),
                                        "tool_name": tool_name.clone(),
                                        "result": result,
                                        "success": true,
                                        "duration_ms": duration_ms,
                                        "iteration": iteration + 1,
                                        "conversation_id": base_conversation_id.clone(),
                                        "message_id": base_message_id.clone(),
                                        "timestamp_ms": timestamp_ms,
                                    }),
                                    timestamp_ms,
                                ));

                                let tool_result_payload = json!({
                                    "type": "tool_result",
                                    "tool": tool_name,
                                    "execution_id": execution_id,
                                    "result": result_for_message,
                                });
                                let tool_result_text =
                                    serde_json::to_string(&tool_result_payload)
                                        .unwrap_or_else(|_| tool_result_payload.to_string());
                                messages.push(LlmMessage {
                                    role: "user".to_string(),
                                    content: json!(tool_result_text),
                                });
                            }
                            Err(err) => {
                                let error_message = err.message;
                                let timestamp_ms = Utc::now().timestamp_millis();
                                self.event_bus.publish(AgentEvent::new_with_timestamp(
                                    EVENT_TOOL_EXECUTION_COMPLETED,
                                    json!({
                                        "execution_id": execution_id,
                                        "tool_name": tool_name,
                                        "success": false,
                                        "error": error_message.clone(),
                                        "duration_ms": duration_ms,
                                        "iteration": iteration + 1,
                                        "conversation_id": base_conversation_id.clone(),
                                        "message_id": base_message_id.clone(),
                                        "timestamp_ms": timestamp_ms,
                                    }),
                                    timestamp_ms,
                                ));
                                return Err(error_message);
                            }
                        }
                    }
                }
            }
        }

        Err("Tool loop exceeded max iterations".to_string())
    }

    fn build_system_prompt(&self, base_system_prompt: Option<&str>) -> String {
        let mut prompt = String::new();
        if let Some(base) = base_system_prompt {
            if !base.trim().is_empty() {
                prompt.push_str(base.trim());
                prompt.push_str("\n\n");
            }
        }

        let tool_list = self.registry.prompt_json();
        let tool_list_str = serde_json::to_string(&tool_list).unwrap_or_else(|_| "[]".to_string());

        prompt.push_str(
            "You are a tool-using agent. Respond ONLY with a single JSON object.\n\n\
Valid response formats:\n\
1) Direct response:\n\
{\"type\":\"direct_response\",\"content\":\"...\"}\n\
2) Tool calls:\n\
{\"type\":\"tool_calls\",\"calls\":[{\"tool\":\"<name>\",\"args\":{...}}]}\n\n\
Available tools:\n",
        );
        prompt.push_str(&tool_list_str);
        prompt.push_str(
            "\n\nTool results will arrive as JSON strings in user messages. Example:\n\
{\"type\":\"tool_result\",\"tool\":\"<name>\",\"execution_id\":\"...\",\"result\":{...}}\n\n\
Do not wrap output in markdown or code fences.",
        );

        prompt
    }
}

fn parse_agent_action(raw: &str) -> Result<AgentAction, String> {
    let json_str = extract_json(raw);
    serde_json::from_str(&json_str)
        .map_err(|err| format!("Failed to parse agent action JSON: {err}"))
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
