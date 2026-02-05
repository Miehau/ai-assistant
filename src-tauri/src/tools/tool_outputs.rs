use crate::db::Db;
use crate::tool_outputs::read_tool_output;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register_tool_output_tools(registry: &mut ToolRegistry, _db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "tool_outputs.read".to_string(),
        description: "Read a stored tool output by id from app data.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "conversation_id": { "type": "string" }
            },
            "required": ["id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object"
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
        if id.is_empty() {
            return Err(ToolError::new("Missing 'id'"));
        }

        let record = read_tool_output(id).map_err(ToolError::new)?;

        if let Some(expected) = args.get("conversation_id").and_then(|v| v.as_str()) {
            if let Some(actual) = record.conversation_id.as_ref() {
                if actual != expected {
                    return Err(ToolError::new(
                        "conversation_id does not match stored record",
                    ));
                }
            } else {
                return Err(ToolError::new("Stored output missing conversation_id"));
            }
        }

        serde_json::to_value(record)
            .map_err(|err| ToolError::new(format!("Failed to serialize tool output record: {err}")))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
