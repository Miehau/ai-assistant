use crate::db::{Db, PreferenceOperations};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register_pref_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    register_get_tool(registry, db.clone())?;
    register_set_tool(registry, db)?;
    Ok(())
}

fn register_get_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "prefs.get".to_string(),
        description: "Get a user preference by key.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "key": { "type": "string" }
            },
            "required": ["key"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "key": { "type": "string" },
                "value": { "type": ["string", "null"] }
            },
            "required": ["key", "value"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let key = require_string_arg(&args, "key")?;
        let value = PreferenceOperations::get_preference(&db, &key)
            .map_err(|err| ToolError::new(format!("Failed to read preference: {err}")))?;
        Ok(json!({
            "key": key,
            "value": value
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

fn register_set_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "prefs.set".to_string(),
        description: "Set a user preference value.".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "key": { "type": "string" },
                "value": { "type": "string" }
            },
            "required": ["key", "value"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "key": { "type": "string" },
                "value": { "type": "string" },
                "updated": { "type": "boolean" }
            },
            "required": ["key", "value", "updated"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let key = require_string_arg(&args, "key")?;
        let value = require_string_arg(&args, "value")?;
        PreferenceOperations::set_preference(&handler_db, &key, &value)
            .map_err(|err| ToolError::new(format!("Failed to set preference: {err}")))?;
        Ok(json!({
            "key": key,
            "value": value,
            "updated": true
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let key = require_string_arg(&args, "key")?;
        let value = require_string_arg(&args, "value")?;
        let existing = PreferenceOperations::get_preference(&preview_db, &key)
            .map_err(|err| ToolError::new(format!("Failed to read preference: {err}")))?;
        let changed = existing.as_deref() != Some(value.as_str());
        Ok(json!({
            "key": key,
            "old_value": existing,
            "new_value": value,
            "changed": changed
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}

fn require_string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing or invalid '{key}'")))
}
