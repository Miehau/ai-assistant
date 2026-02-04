use jsonschema::JSONSchema;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use uuid::Uuid;

mod files;
mod prefs;
mod search;
mod vault;
mod web;
mod integrations;
mod tool_outputs;
mod approvals;

pub use files::register_file_tools;
pub use prefs::register_pref_tools;
pub use search::register_search_tool;
pub use web::register_web_tools;
pub use integrations::register_integration_tools;
pub use tool_outputs::register_tool_output_tools;
pub use approvals::{
    get_tool_approval_override,
    load_tool_approval_overrides,
    set_tool_approval_override,
    PREF_TOOL_APPROVAL_OVERRIDES,
};

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

#[derive(Clone)]
pub struct ApprovalStore {
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<ToolApprovalDecision>>>>,
}

#[derive(Clone, Debug)]
pub enum ToolApprovalDecision {
    Approved,
    Denied,
}

impl ApprovalStore {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_request(&self) -> (String, mpsc::Receiver<ToolApprovalDecision>) {
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
            ToolApprovalDecision::Approved
        } else {
            ToolApprovalDecision::Denied
        };
        sender
            .send(decision)
            .map_err(|_| "Failed to deliver approval decision".to_string())
    }

    pub fn cancel(&self, approval_id: &str) -> Result<(), String> {
        let removed = {
            let mut pending = self.pending.lock().unwrap();
            pending.remove(approval_id)
        };
        if removed.is_some() {
            Ok(())
        } else {
            Err(format!("Unknown approval id: {approval_id}"))
        }
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

        let read_range = call_tool(
            &registry,
            "files.read_range",
            json!({ "path": "notes/test.md", "start_line": 2, "max_lines": 1 }),
        );
        let range_content = read_range
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(range_content.contains("Universe"));

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

    #[test]
    fn read_range_defaults_and_limits() {
        let vault_root = std::env::temp_dir().join(format!("vault-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault_root).expect("vault root create failed");

        let db = setup_db(vault_root.to_str().unwrap());
        let mut registry = ToolRegistry::new();
        register_file_tools(&mut registry, db.clone()).expect("file tools registration failed");

        call_tool(
            &registry,
            "files.create",
            json!({
                "path": "notes/range.md",
                "content": "Line1\nLine2\nLine3\nLine4\nLine5\n"
            }),
        );

        let read_default = call_tool(
            &registry,
            "files.read_range",
            json!({ "path": "notes/range.md" }),
        );
        let default_content = read_default
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(default_content.contains("Line1"));
        assert!(default_content.contains("Line5"));

        let read_window = call_tool(
            &registry,
            "files.read_range",
            json!({ "path": "notes/range.md", "start_line": 2, "end_line": 3 }),
        );
        let window_content = read_window
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(!window_content.contains("Line1"));
        assert!(window_content.contains("Line2"));
        assert!(window_content.contains("Line3"));
        assert!(!window_content.contains("Line4"));

        let read_truncated = call_tool(
            &registry,
            "files.read_range",
            json!({ "path": "notes/range.md", "max_chars": 6 }),
        );
        let truncated = read_truncated
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let truncated_content = read_truncated
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(truncated);
        assert!(truncated_content.len() <= 6);
    }

    #[test]
    fn search_replace_updates_content() {
        let vault_root = std::env::temp_dir().join(format!("vault-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault_root).expect("vault root create failed");

        let db = setup_db(vault_root.to_str().unwrap());
        let mut registry = ToolRegistry::new();
        register_file_tools(&mut registry, db.clone()).expect("file tools registration failed");

        call_tool(
            &registry,
            "files.create",
            json!({
                "path": "notes/replace.md",
                "content": "Alpha\nbeta\nAlpha\n"
            }),
        );

        let replace_result = call_tool(
            &registry,
            "files.search_replace",
            json!({
                "path": "notes/replace.md",
                "query": "Alpha",
                "replace": "Gamma"
            }),
        );
        let replacements = replace_result
            .get("replacements")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        assert_eq!(replacements, 2);

        let read_back = call_tool(
            &registry,
            "files.read",
            json!({ "path": "notes/replace.md" }),
        );
        let content = read_back
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(content.contains("Gamma"));
        assert!(content.contains("beta"));

        let replace_case_insensitive = call_tool(
            &registry,
            "files.search_replace",
            json!({
                "path": "notes/replace.md",
                "query": "BETA",
                "replace": "Delta",
                "case_sensitive": false
            }),
        );
        let replacements = replace_case_insensitive
            .get("replacements")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        assert_eq!(replacements, 1);

        let read_back = call_tool(
            &registry,
            "files.read",
            json!({ "path": "notes/replace.md" }),
        );
        let content = read_back
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(content.contains("Delta"));
    }
}
