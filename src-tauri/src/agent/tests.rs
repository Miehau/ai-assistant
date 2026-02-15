use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde_json::{json, Value};

use super::DynamicController;
use crate::db::{ConversationOperations, Db};
use crate::events::EventBus;
use crate::llm::{LlmMessage, StreamResult};
use crate::tools::{
    ApprovalStore, ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry,
    ToolResultMode,
};

fn setup_db() -> Db {
    let db_path = std::env::temp_dir().join(format!("ai-agent-test-{}.db", uuid::Uuid::new_v4()));
    let mut db = Db::new(db_path.to_str().unwrap()).expect("db init failed");
    db.run_migrations().expect("db migrations failed");
    db
}

fn register_echo_tool(registry: &mut ToolRegistry) {
    let metadata = ToolMetadata {
        name: "test.echo".to_string(),
        description: "Echo input text.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" }
            },
            "required": ["text"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" }
            },
            "required": ["text"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(|args: Value, _ctx: ToolExecutionContext| -> Result<Value, ToolError> {
        Ok(json!({
            "text": args.get("text").cloned().unwrap_or(Value::Null)
        }))
    });

    registry
        .register(ToolDefinition {
            metadata,
            handler,
            preview: None,
        })
        .expect("register echo tool");
}

fn build_controller(tool_registry: ToolRegistry) -> DynamicController {
    let db = setup_db();
    db.get_or_create_conversation("conv-1")
        .expect("create conversation");
    let event_bus = EventBus::new();
    let approvals = ApprovalStore::new();
    let cancel_flag = Arc::new(AtomicBool::new(false));

    DynamicController::new(
        db,
        event_bus,
        tool_registry,
        approvals,
        cancel_flag,
        Vec::new(),
        "conv-1".to_string(),
        "msg-1".to_string(),
        "assistant-1".to_string(),
    )
    .expect("build controller")
}

#[test]
fn agents_md_is_index_only() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let agents_path = manifest_dir.join("..").join("AGENTS.md");
    let contents = fs::read_to_string(&agents_path).expect("read AGENTS.md");

    let expected = "# Agent Engineering Guardrails\n\nSee `src-tauri/docs/agent/README.md` for the authoritative guardrails index.\n";
    assert_eq!(contents, expected, "AGENTS.md must be index-only");
}

#[test]
fn agent_docs_index_exists_and_links_provider_contracts() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let docs_path = manifest_dir
        .join("docs")
        .join("agent")
        .join("README.md");
    let contents = fs::read_to_string(&docs_path).expect("read agent docs README");

    assert!(
        contents.contains("provider-contracts.md"),
        "Agent docs index must link provider-contracts.md"
    );
}

#[test]
fn controller_executes_tool_then_complete() {
    let mut registry = ToolRegistry::new();
    register_echo_tool(&mut registry);

    let mut controller = build_controller(registry);
    let mut responses = VecDeque::from(vec![
        json!({
            "action": "next_step",
            "thinking": "ok",
            "tool": "test.echo",
            "args": { "text": "hello" }
        })
        .to_string(),
        json!({
            "action": "complete",
            "message": "done"
        })
        .to_string(),
    ]);

    let mut call_llm = |_: &[LlmMessage], _: Option<&str>, _: Option<Value>| {
        let content = responses.pop_front().expect("missing response");
        Ok(StreamResult { content, usage: None })
    };

    let result = controller
        .run("user message", &mut call_llm)
        .expect("run");
    assert_eq!(result, "done");

    let tool_executions = controller.take_tool_executions();
    assert_eq!(tool_executions.len(), 1);
    assert_eq!(tool_executions[0].tool_name, "test.echo");
}

#[test]
fn controller_drops_oversized_tool_batch() {
    let mut registry = ToolRegistry::new();
    register_echo_tool(&mut registry);

    let mut controller = build_controller(registry);
    controller.test_session_mut().config.max_tool_calls_per_step = 1;
    controller.test_cancel_flag().store(false, Ordering::Relaxed);

    let mut responses = VecDeque::from(vec![
        json!({
            "action": "next_step",
            "thinking": "ok",
            "tools": [
                { "tool": "test.echo", "args": { "text": "one" } },
                { "tool": "test.echo", "args": { "text": "two" } }
            ]
        })
        .to_string(),
        json!({
            "action": "complete",
            "message": "done"
        })
        .to_string(),
    ]);

    let mut call_llm = |_: &[LlmMessage], _: Option<&str>, _: Option<Value>| {
        let content = responses.pop_front().expect("missing response");
        Ok(StreamResult { content, usage: None })
    };

    let _ = controller
        .run("user message", &mut call_llm)
        .expect("run");

    let last = controller
        .test_session()
        .step_results
        .last()
        .expect("step result");
    let output = last.output.as_ref().expect("output");
    assert_eq!(
        output.get("dropped_calls").and_then(|v| v.as_i64()),
        Some(1)
    );
    assert_eq!(
        output.get("executed_calls").and_then(|v| v.as_i64()),
        Some(1)
    );
}

#[test]
fn controller_rejects_invalid_args() {
    let mut registry = ToolRegistry::new();
    register_echo_tool(&mut registry);

    let mut controller = build_controller(registry);
    let mut responses = VecDeque::from(vec![
        json!({
            "action": "next_step",
            "thinking": "ok",
            "tool": "test.echo",
            "args": {}
        })
        .to_string(),
        json!({
            "action": "complete",
            "message": "done"
        })
        .to_string(),
    ]);

    let mut call_llm = |_: &[LlmMessage], _: Option<&str>, _: Option<Value>| {
        let content = responses.pop_front().expect("missing response");
        Ok(StreamResult { content, usage: None })
    };

    let _ = controller
        .run("user message", &mut call_llm)
        .expect("run");

    let last = controller
        .test_session()
        .step_results
        .last()
        .expect("step result");
    assert!(!last.success);
    assert!(
        last.error
            .as_deref()
            .unwrap_or("")
            .contains("Invalid args for tool test.echo"),
        "expected preflight args validation error"
    );
}
