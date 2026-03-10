use super::{
    is_private_host, load_allowlist, normalize_host_from_input, require_string_arg, save_allowlist,
    AllowedHost,
};
use crate::db::Db;
use crate::tools::{ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode};
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn register_approve_tool(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "web.approve_domain".to_string(),
        description: "Approve a website host for automated access (exact host).".to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "allow_private": { "type": "boolean" }
            },
            "required": ["url"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "host": { "type": "string" },
                "allow_private": { "type": "boolean" },
                "saved": { "type": "boolean" }
            },
            "required": ["host", "allow_private", "saved"],
            "additionalProperties": false
        }),
        requires_approval: true,
        result_mode: ToolResultMode::Inline,
    };

    let handler_db = db.clone();
    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let url = require_string_arg(&args, "url")?;
        let allow_private = args
            .get("allow_private")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let host = normalize_host_from_input(&url)?;

        if is_private_host(&host) && !allow_private {
            return Err(ToolError::new(
                "Private/local hosts require allow_private=true",
            ));
        }

        let mut allowed = load_allowlist(&handler_db)?;
        let now = chrono::Utc::now().timestamp();
        if let Some(entry) = allowed.iter_mut().find(|entry| entry.host == host) {
            entry.allow_private = allow_private;
            entry.approved_at = now;
        } else {
            allowed.push(AllowedHost {
                host: host.clone(),
                allow_private,
                approved_at: now,
            });
        }
        save_allowlist(&handler_db, &allowed)?;

        Ok(json!({
            "host": host,
            "allow_private": allow_private,
            "saved": true
        }))
    });

    let preview_db = db;
    let preview = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let url = require_string_arg(&args, "url")?;
        let allow_private = args
            .get("allow_private")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let host = normalize_host_from_input(&url)?;
        let host_for_preview = host.clone();
        let mut preview = json!({
            "host": host_for_preview,
            "allow_private": allow_private
        });
        if let Ok(existing) = load_allowlist(&preview_db) {
            if let Some(entry) = existing.iter().find(|entry| entry.host == host) {
                if let Some(obj) = preview.as_object_mut() {
                    obj.insert(
                        "existing".to_string(),
                        json!({ "allow_private": entry.allow_private, "approved_at": entry.approved_at }),
                    );
                }
            }
        }
        Ok(preview)
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: Some(preview),
    })
}
