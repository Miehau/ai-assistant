use crate::agent::DynamicController;
use crate::db::{CustomBackendOperations, Db, ModelOperations};
use crate::events::EventBus;
use crate::llm::{create_provider, LlmMessage, ProviderConfig, StreamResult};
use crate::tools::{
    ApprovalStore, ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry,
    ToolResultMode,
};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const SUB_AGENT_HTTP_TIMEOUT_SECS: u64 = 120;
const SUB_AGENT_HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;

pub fn register_subagent_tools(
    registry: &mut ToolRegistry,
    db: Db,
    event_bus: EventBus,
    tool_registry: ToolRegistry,
    approvals: ApprovalStore,
) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "agent.spawn".to_string(),
        description: "Spawn a sub-agent to handle a focused task independently. \
            The sub-agent gets its own context window and can use all available tools. \
            Use this to delegate work (e.g. classify a file, summarize a document) \
            without polluting the main conversation context. \
            Returns the sub-agent's final response."
            .to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task instruction for the sub-agent"
                },
                "context": {
                    "type": "string",
                    "description": "Optional additional context (e.g. file contents, data)"
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override (defaults to parent's model)"
                }
            },
            "required": ["prompt"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "response": { "type": "string" },
                "success": { "type": "boolean" }
            },
            "required": ["session_id", "response", "success"],
            "additionalProperties": false
        }),
        requires_approval: true,
        // Sub-agent runs a full controller loop (many LLM turns), so it can
        // take much longer than the default 120 s tool timeout. Using Persist
        // result mode is fine — the orchestrator's cancel_flag is checked
        // inside the sub-agent loop for responsive cancellation.
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(move |args: Value, ctx: ToolExecutionContext| {
        if ctx.is_sub_agent {
            return Err(ToolError::new(
                "Sub-agents cannot spawn other sub-agents (nesting is not allowed)",
            ));
        }

        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing or invalid 'prompt'"))?
            .to_string();

        let context = args
            .get("context")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let model_override = args
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let provider = ctx
            .provider
            .as_deref()
            .ok_or_else(|| ToolError::new("No provider available in execution context"))?
            .to_string();

        let model = model_override.unwrap_or_else(|| {
            ctx.model
                .as_deref()
                .unwrap_or("claude-sonnet-4-6")
                .to_string()
        });

        let conversation_id = ctx
            .conversation_id
            .as_deref()
            .ok_or_else(|| ToolError::new("No conversation_id in execution context"))?
            .to_string();

        let parent_session_id = ctx
            .session_id
            .as_deref()
            .ok_or_else(|| ToolError::new("No session_id in execution context"))?
            .to_string();

        let cancel_flag = ctx
            .cancel_flag
            .as_ref()
            .ok_or_else(|| ToolError::new("No cancel flag in execution context"))?
            .clone();

        // Clone conversation_id before it gets moved into controller
        let conversation_id_for_options = conversation_id.clone();

        // Build the user message for the sub-agent
        let user_message = if let Some(ref ctx_text) = context {
            format!("{}\n\nCONTEXT:\n{}", prompt, ctx_text)
        } else {
            prompt.clone()
        };

        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: json!(user_message),
        }];

        // Build the sub-agent's DynamicController
        let mut controller = DynamicController::new(
            db.clone(),
            event_bus.clone(),
            tool_registry.clone(),
            approvals.clone(),
            cancel_flag.clone(),
            messages,
            conversation_id,
            uuid::Uuid::new_v4().to_string(), // message_id (sub-agent user msg)
            uuid::Uuid::new_v4().to_string(), // assistant_message_id
        )
        .map_err(|e| ToolError::new(format!("Failed to create sub-agent: {e}")))?;

        controller.set_provider_info(provider.clone(), model.clone());
        controller.set_parent_session_id(parent_session_id);
        let session_id = controller.session_id().to_string();

        log::info!(
            "[subagent] spawned: session_id={} parent_session={} provider={} model={}",
            session_id,
            ctx.session_id.as_deref().unwrap_or("none"),
            provider,
            model,
        );

        // Build the call_llm closure for the sub-agent
        let api_key = resolve_api_key(&db, &provider)?;
        let base_url = if provider == "custom" {
            resolve_custom_backend(&db, &provider).map(|(url, _)| url)
        } else {
            None
        };

        let llm_provider = create_provider(ProviderConfig {
            provider_name: provider.clone(),
            model: model.clone(),
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
            base_url,
        })
        .map_err(|e| ToolError::new(format!("Failed to create provider: {e}")))?;

        let client = build_sub_agent_http_client();
        let request_options = llm_provider.build_request_options(&conversation_id_for_options, "controller");

        let mut call_llm = |messages: &[LlmMessage],
                             system_prompt: Option<&str>,
                             output_format: Option<Value>|
         -> Result<StreamResult, String> {
            if cancel_flag.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }

            llm_provider.controller_call(
                &client,
                messages,
                system_prompt,
                output_format,
                Some(&request_options),
            )
        };

        // Run the sub-agent
        match controller.run(&user_message, &mut call_llm) {
            Ok(response) => {
                log::info!(
                    "[subagent] completed: session_id={} response_chars={}",
                    session_id,
                    response.chars().count()
                );
                Ok(json!({
                    "session_id": session_id,
                    "response": response,
                    "success": true
                }))
            }
            Err(error) => {
                log::warn!(
                    "[subagent] failed: session_id={} error={}",
                    session_id,
                    error
                );
                Ok(json!({
                    "session_id": session_id,
                    "response": format!("Sub-agent error: {error}"),
                    "success": false
                }))
            }
        }
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn resolve_api_key(db: &Db, provider: &str) -> Result<String, ToolError> {
    match provider {
        "claude_cli" | "ollama" | "custom" => Ok(String::new()),
        _ => {
            let key = ModelOperations::get_api_key(db, provider)
                .ok()
                .flatten()
                .unwrap_or_default();
            if key.is_empty() {
                return Err(ToolError::new(format!(
                    "No API key configured for provider '{provider}'"
                )));
            }
            Ok(key)
        }
    }
}

fn resolve_custom_backend(db: &Db, provider: &str) -> Option<(String, Option<String>)> {
    if provider != "custom" {
        return None;
    }
    CustomBackendOperations::get_custom_backends(db)
        .ok()
        .and_then(|backends: Vec<crate::db::CustomBackend>| backends.into_iter().next())
        .map(|backend| (backend.url, backend.api_key))
}

fn build_sub_agent_http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(SUB_AGENT_HTTP_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(SUB_AGENT_HTTP_CONNECT_TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| Client::new())
}
