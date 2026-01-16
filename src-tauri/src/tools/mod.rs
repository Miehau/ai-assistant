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
use std::collections::{HashMap, HashSet};
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

pub type ToolHandler = dyn Fn(Value, ToolExecutionContext) -> Result<Value, ToolError> + Send + Sync;
pub type ToolPreviewHandler =
    dyn Fn(Value, ToolExecutionContext) -> Result<Value, ToolError> + Send + Sync;

#[derive(Clone, Debug, Default)]
pub struct ToolExecutionContext;

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
    pub tool_executions: Vec<ToolExecutionRecord>,
}

#[derive(Clone, Debug)]
pub struct ToolExecutionRecord {
    pub execution_id: String,
    pub tool_name: String,
    pub args: Value,
    pub result: Option<Value>,
    pub success: bool,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub iteration: usize,
    pub timestamp_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AgentAction {
    DirectResponse { content: String },
    ToolCalls { calls: Vec<ToolCall> },
    Plan { steps: Vec<String> },
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
        F: FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>,
    {
        fn log_tool_event(label: &str, payload: &Value) {
            let payload_text = serde_json::to_string(payload).unwrap_or_else(|_| payload.to_string());
            println!("[tool] {}: {}", label, payload_text);
        }

        let system_prompt = self.build_system_prompt(base_system_prompt);
        let base_conversation_id = context.conversation_id.clone();
        let base_message_id = context.message_id.clone();
        let mut tool_executions: Vec<ToolExecutionRecord> = Vec::new();
        let mut completed_calls: HashSet<String> = HashSet::new();
        let mut call_attempts: HashMap<String, usize> = HashMap::new();
        let mut call_errors: HashMap<String, String> = HashMap::new();
        let mut force_direct_response: Option<String> = None;
        let max_identical_attempts: usize = 2;

        let mut tool_iterations = 0usize;
        let mut plan_seen = false;
        let mut plan_reminders = 0u8;

        while tool_iterations < self.config.max_iterations {
            let response = call_llm(&messages, Some(&system_prompt), None)?;
            let action = parse_agent_action(&response.content)?;

            match action {
                AgentAction::DirectResponse { content } => {
                    return Ok(ToolLoopResponse {
                        content,
                        tool_executions,
                    });
                }
                AgentAction::Plan { steps } => {
                    if let Some(reason) = force_direct_response.as_ref() {
                        messages.push(LlmMessage {
                            role: "user".to_string(),
                            content: json!(format!(
                                "Respond only with a direct_response and ask for help. Reason: {}",
                                reason
                            )),
                        });
                        continue;
                    }
                    plan_seen = true;
                    let plan_payload = json!({ "type": "plan", "steps": steps });
                    let plan_text =
                        serde_json::to_string(&plan_payload).unwrap_or_else(|_| plan_payload.to_string());
                    messages.push(LlmMessage {
                        role: "assistant".to_string(),
                        content: json!(plan_text),
                    });
                    continue;
                }
                AgentAction::ToolCalls { calls } => {
                    if let Some(reason) = force_direct_response.as_ref() {
                        messages.push(LlmMessage {
                            role: "user".to_string(),
                            content: json!(format!(
                                "Respond only with a direct_response and ask for help. Reason: {}",
                                reason
                            )),
                        });
                        continue;
                    }
                    if calls.is_empty() {
                        return Err("No tool calls provided by model".to_string());
                    }

                    if !plan_seen {
                        if plan_reminders < 1 {
                            plan_reminders += 1;
                            messages.push(LlmMessage {
                                role: "user".to_string(),
                                content: json!(
                                    "Please provide a plan first using {\"type\":\"plan\",\"steps\":[...]}. Then call one tool."
                                ),
                            });
                            continue;
                        }
                    }

                    if calls.len() > 1 {
                        println!(
                            "[tool] warning: received {} tool calls; executing only the first",
                            calls.len()
                        );
                    }

                    let call = calls.into_iter().next().unwrap();
                    tool_iterations += 1;

                    {
                        let tool = self
                            .registry
                            .get(&call.tool)
                            .ok_or_else(|| format!("Unknown tool: {}", call.tool))?;
                        let tool_name = tool.metadata.name.clone();
                        let call_args = normalize_tool_args(call.args)?;
                        let call_key = serde_json::to_string(&json!({
                            "tool": tool_name,
                            "args": call_args
                        }))
                        .unwrap_or_default();

                        self.registry
                            .validate_args(&tool.metadata, &call_args)
                            .map_err(|err| err.message)?;

                        if completed_calls.contains(&call_key) {
                            messages.push(LlmMessage {
                                role: "user".to_string(),
                                content: json!(
                                    "That tool call already succeeded with the same arguments. Do not repeat it. If more work is needed, choose a different tool or respond with a direct_response."
                                ),
                            });
                            continue;
                        }
                        let attempts_so_far = *call_attempts.get(&call_key).unwrap_or(&0);
                        if attempts_so_far >= max_identical_attempts {
                            let last_error = call_errors
                                .get(&call_key)
                                .cloned()
                                .unwrap_or_else(|| "Unknown error".to_string());
                            let reason = format!(
                                "Repeated tool call failed {} times with identical arguments. Last error: {}",
                                attempts_so_far,
                                last_error
                            );
                            force_direct_response = Some(reason.clone());
                            messages.push(LlmMessage {
                                role: "user".to_string(),
                                content: json!(format!(
                                    "The same tool call has failed multiple times. Ask the user for help and include the last error. Error: {}",
                                    last_error
                                )),
                            });
                            continue;
                        }
                        let attempt_count = attempts_so_far + 1;
                        call_attempts.insert(call_key.clone(), attempt_count);

                        let execution_id = Uuid::new_v4().to_string();
                        let tool_context = ToolExecutionContext;
                        log_tool_event(
                            "call",
                            &json!({
                                "execution_id": execution_id,
                                "tool_name": tool_name,
                                "args": call_args,
                                    "iteration": tool_iterations,
                                "conversation_id": base_conversation_id.clone(),
                                "message_id": base_message_id.clone(),
                            }),
                        );

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
                                    "iteration": tool_iterations,
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
                                    log_tool_event(
                                        "approval",
                                        &json!({
                                            "execution_id": execution_id,
                                            "tool_name": tool_name,
                                            "approved": true,
                                            "iteration": tool_iterations,
                                            "conversation_id": base_conversation_id.clone(),
                                            "message_id": base_message_id.clone(),
                                        }),
                                    );
                                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                                        EVENT_TOOL_EXECUTION_APPROVED,
                                        json!({
                                            "execution_id": execution_id.clone(),
                                            "approval_id": approval_id.clone(),
                                            "tool_name": tool_name.clone(),
                                            "iteration": tool_iterations,
                                            "conversation_id": base_conversation_id.clone(),
                                            "message_id": base_message_id.clone(),
                                            "timestamp_ms": timestamp_ms,
                                        }),
                                        timestamp_ms,
                                    ));
                                }
                                ApprovalDecision::Denied => {
                                    log_tool_event(
                                        "approval",
                                        &json!({
                                            "execution_id": execution_id,
                                            "tool_name": tool_name,
                                            "approved": false,
                                            "iteration": tool_iterations,
                                            "conversation_id": base_conversation_id.clone(),
                                            "message_id": base_message_id.clone(),
                                        }),
                                    );
                                    self.event_bus.publish(AgentEvent::new_with_timestamp(
                                        EVENT_TOOL_EXECUTION_DENIED,
                                        json!({
                                            "execution_id": execution_id.clone(),
                                            "approval_id": approval_id.clone(),
                                            "tool_name": tool_name.clone(),
                                            "iteration": tool_iterations,
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
                                "iteration": tool_iterations,
                                "conversation_id": base_conversation_id.clone(),
                                "message_id": base_message_id.clone(),
                                "timestamp_ms": timestamp_ms,
                            }),
                            timestamp_ms,
                        ));

                        let start = Instant::now();
                        let result = (tool.handler)(call_args.clone(), tool_context);
                        let duration_ms = start.elapsed().as_millis() as i64;
                        let completed_at = Utc::now().timestamp_millis();

                        match result {
                            Ok(result) => {
                                let result_for_message = result.clone();
                                let timestamp_ms = completed_at;
                                tool_executions.push(ToolExecutionRecord {
                                    execution_id: execution_id.clone(),
                                    tool_name: tool_name.clone(),
                                    args: call_args.clone(),
                                    result: Some(result.clone()),
                                    success: true,
                                    error: None,
                                    duration_ms,
                                    iteration: tool_iterations,
                                    timestamp_ms,
                                });
                                log_tool_event(
                                    "response",
                                    &json!({
                                        "execution_id": execution_id,
                                        "tool_name": tool_name,
                                        "success": true,
                                        "duration_ms": duration_ms,
                                        "result": result_for_message,
                                        "iteration": tool_iterations,
                                        "conversation_id": base_conversation_id.clone(),
                                        "message_id": base_message_id.clone(),
                                    }),
                                );
                                self.event_bus.publish(AgentEvent::new_with_timestamp(
                                    EVENT_TOOL_EXECUTION_COMPLETED,
                                    json!({
                                        "execution_id": execution_id.clone(),
                                        "tool_name": tool_name.clone(),
                                        "result": result,
                                        "success": true,
                                        "duration_ms": duration_ms,
                                        "iteration": tool_iterations,
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
                                    "success": true,
                                    "attempt": attempt_count,
                                    "result": result_for_message,
                                });
                                let tool_result_text =
                                    serde_json::to_string(&tool_result_payload)
                                        .unwrap_or_else(|_| tool_result_payload.to_string());
                                messages.push(LlmMessage {
                                    role: "user".to_string(),
                                    content: json!(tool_result_text),
                                });
                                completed_calls.insert(call_key.clone());
                                call_errors.remove(&call_key);
                            }
                            Err(err) => {
                                let error_message = err.message;
                                let timestamp_ms = completed_at;
                                tool_executions.push(ToolExecutionRecord {
                                    execution_id: execution_id.clone(),
                                    tool_name: tool_name.clone(),
                                    args: call_args,
                                    result: None,
                                    success: false,
                                    error: Some(error_message.clone()),
                                    duration_ms,
                                    iteration: tool_iterations,
                                    timestamp_ms,
                                });
                                log_tool_event(
                                    "response",
                                    &json!({
                                        "execution_id": execution_id,
                                        "tool_name": tool_name,
                                        "success": false,
                                        "duration_ms": duration_ms,
                                        "error": error_message,
                                        "iteration": tool_iterations,
                                        "conversation_id": base_conversation_id.clone(),
                                        "message_id": base_message_id.clone(),
                                    }),
                                );
                                self.event_bus.publish(AgentEvent::new_with_timestamp(
                                    EVENT_TOOL_EXECUTION_COMPLETED,
                                    json!({
                                        "execution_id": execution_id.clone(),
                                        "tool_name": tool_name.clone(),
                                        "success": false,
                                        "error": error_message.clone(),
                                        "duration_ms": duration_ms,
                                        "iteration": tool_iterations,
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
                                    "success": false,
                                    "attempt": attempt_count,
                                    "error": error_message,
                                });
                                let tool_result_text =
                                    serde_json::to_string(&tool_result_payload)
                                        .unwrap_or_else(|_| tool_result_payload.to_string());
                                messages.push(LlmMessage {
                                    role: "user".to_string(),
                                    content: json!(tool_result_text),
                                });
                                call_errors.insert(call_key, error_message);
                                continue;
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

        prompt.push_str(
            "Tool usage guidance:\n\
- If creating a new file with known content, use `files.create` with the `content` field.\n\
- Avoid a separate `files.write` call unless you are modifying an existing file.\n\
- When tools are needed, respond with a plan first, then execute only the next step.\n\
- After a successful tool_result, check if the user request is satisfied and respond with a direct response.\n\
- Never repeat an identical tool call (same tool + args) after it succeeds.\n\
- If a tool_result reports success=false, you may retry once with corrected arguments. If it fails again, ask the user for help and mention the error.\n\
- Only one tool call per response.\n\n",
        );

        let tool_list = self.registry.prompt_json();
        let tool_list_str = serde_json::to_string(&tool_list).unwrap_or_else(|_| "[]".to_string());

        prompt.push_str(
            "You are a tool-using agent. Respond ONLY with a single JSON object.\n\n\
Valid response formats:\n\
1) Direct response:\n\
{\"type\":\"direct_response\",\"content\":\"...\"}\n\
2) Plan:\n\
{\"type\":\"plan\",\"steps\":[\"step 1\", \"step 2\", \"...\"]}\n\
3) Tool call (single only):\n\
{\"type\":\"tool_calls\",\"calls\":[{\"tool\":\"<name>\",\"args\":\"{...}\"}]}\n\
Note: \"args\" MUST be a JSON string containing the tool arguments object.\n\n\
STRICT JSON OUTPUT RULES:\n\
- Output must be a single JSON object and nothing else.\n\
- Do not include preambles, explanations, or trailing text.\n\
- Do not wrap in code fences or markdown.\n\
- Do not include comments.\n\
- If you must choose, prefer {\"type\":\"direct_response\"}.\n\n\
Example (valid):\n\
{\"type\":\"tool_calls\",\"calls\":[{\"tool\":\"files.create\",\"args\":\"{\\\"path\\\":\\\"notes/todo.md\\\",\\\"content\\\":\\\"- Buy milk\\\\n\\\"}\"}]}\n\n\
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
    let mut deserializer = serde_json::Deserializer::from_str(&json_str);
    let action = AgentAction::deserialize(&mut deserializer)
        .map_err(|err| format!("Failed to parse agent action JSON: {err}"))?;
    if deserializer.end().is_err() {
        println!("[tool] warning: trailing characters after JSON action");
    }
    Ok(action)
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

fn normalize_tool_args(args: Value) -> Result<Value, String> {
    match args {
        Value::Null => Ok(json!({})),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(json!({}));
            }
            serde_json::from_str(trimmed)
                .map_err(|err| format!("Failed to parse tool args JSON string: {err}"))
        }
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::{register_file_tools, register_search_tool, ToolExecutionContext, ToolRegistry};
    use crate::db::{Db, PreferenceOperations};
    use serde_json::json;
    use std::fs;
    use std::process::Command;
    use uuid::Uuid;

    fn setup_db(vault_root: &str) -> Db {
        let db_path = std::env::temp_dir().join(format!("vault-tools-{}.db", Uuid::new_v4()));
        let mut db = Db::new(db_path.to_str().unwrap()).expect("db init failed");
        db.run_migrations().expect("db migrations failed");
        db.set_preference("plugins.files.vault_root", vault_root)
            .expect("set vault root failed");
        db
    }

    fn call_tool(registry: &ToolRegistry, name: &str, args: serde_json::Value) -> serde_json::Value {
        let tool = registry.get(name).expect("missing tool");
        let ctx = ToolExecutionContext;
        (tool.handler)(args, ctx).expect("tool execution failed")
    }

    fn rg_available() -> bool {
        Command::new("rg")
            .arg("--version")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[test]
    fn vault_file_tools_and_search_smoke() {
        let vault_root = std::env::temp_dir().join(format!("vault-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault_root).expect("vault root create failed");

        let db = setup_db(vault_root.to_str().unwrap());
        let mut registry = ToolRegistry::new();
        register_file_tools(&mut registry, db.clone()).expect("file tools registration failed");
        register_search_tool(&mut registry, db.clone()).expect("search tool registration failed");

        call_tool(
            &registry,
            "files.create",
            json!({
                "path": "notes/test.md",
                "content": "Hello\nWorld\n"
            }),
        );

        let read = call_tool(
            &registry,
            "files.read",
            json!({ "path": "notes/test.md" }),
        );
        assert!(read
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("World"));

        call_tool(
            &registry,
            "files.append",
            json!({
                "path": "notes/test.md",
                "content": "Append\n"
            }),
        );

        call_tool(
            &registry,
            "files.edit",
            json!({
                "path": "notes/test.md",
                "start_line": 2,
                "end_line": 2,
                "content": "Universe"
            }),
        );

        let read_updated = call_tool(
            &registry,
            "files.read",
            json!({ "path": "notes/test.md" }),
        );
        let updated_content = read_updated
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(updated_content.contains("Universe"));
        assert!(updated_content.contains("Append"));

        if rg_available() {
            let search = call_tool(
                &registry,
                "search.rg",
                json!({
                    "query": "Universe",
                    "max_results": 5
                }),
            );
            let results_len = search
                .get("results")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            assert!(results_len > 0);
        }
    }
}
