use crate::tool_outputs::{tool_outputs_root, ToolOutputRecord};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn register_list_tool(registry: &mut ToolRegistry) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "tool_outputs.list".to_string(),
        description: "List stored tool outputs with filtering, sorting, and preview capabilities."
            .to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "conversation_id": {
                    "type": "string",
                    "description": "Filter by conversation ID"
                },
                "tool_name": {
                    "type": "string",
                    "description": "Filter by tool name"
                },
                "success": {
                    "type": "boolean",
                    "description": "Filter by success status"
                },
                "after": {
                    "type": "integer",
                    "description": "Unix timestamp - only show outputs created after this time"
                },
                "before": {
                    "type": "integer",
                    "description": "Unix timestamp - only show outputs created before this time"
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "default": 20,
                    "description": "Maximum number of results"
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "default": 0,
                    "description": "Number of results to skip"
                },
                "sort_by": {
                    "type": "string",
                    "enum": ["created_at", "size", "tool_name"],
                    "default": "created_at",
                    "description": "Sort field"
                },
                "sort_order": {
                    "type": "string",
                    "enum": ["asc", "desc"],
                    "default": "desc",
                    "description": "Sort order"
                },
                "include_preview": {
                    "type": "boolean",
                    "default": true,
                    "description": "Include preview of output data"
                },
                "preview_length": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 500,
                    "default": 100,
                    "description": "Characters to include in preview"
                }
            },
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "outputs": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "tool_name": { "type": "string" },
                            "conversation_id": { "type": ["string", "null"] },
                            "message_id": { "type": "string" },
                            "created_at": { "type": "integer" },
                            "success": { "type": "boolean" },
                            "size_bytes": { "type": "integer" },
                            "preview": { "type": "string" },
                            "summary": {
                                "type": "object",
                                "properties": {
                                    "type": { "type": "string" },
                                    "keys": { "type": "integer" },
                                    "items": { "type": "integer" }
                                }
                            }
                        },
                        "required": ["id", "tool_name", "created_at", "success", "size_bytes"]
                    }
                },
                "total": {
                    "type": "integer",
                    "description": "Total matching outputs"
                },
                "has_more": {
                    "type": "boolean",
                    "description": "Whether more results exist"
                }
            },
            "required": ["outputs", "total", "has_more"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let root = tool_outputs_root().map_err(ToolError::new)?;
        if !root.exists() {
            return Ok(json!({ "outputs": [], "total": 0, "has_more": false }));
        }

        // Read filters
        let filter_conversation_id = args.get("conversation_id").and_then(|v| v.as_str());
        let filter_tool_name = args.get("tool_name").and_then(|v| v.as_str());
        let filter_success = args.get("success").and_then(|v| v.as_bool());
        let filter_after = args.get("after").and_then(|v| v.as_i64());
        let filter_before = args.get("before").and_then(|v| v.as_i64());
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(100) as usize;
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let sort_by = args
            .get("sort_by")
            .and_then(|v| v.as_str())
            .unwrap_or("created_at");
        let sort_order = args
            .get("sort_order")
            .and_then(|v| v.as_str())
            .unwrap_or("desc");
        let include_preview = args
            .get("include_preview")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let preview_length = args
            .get("preview_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(100)
            .min(500) as usize;

        // Read all .json files and deserialize
        let entries = std::fs::read_dir(&root)
            .map_err(|e| ToolError::new(format!("Failed to read tool outputs directory: {e}")))?;

        struct ListEntry {
            record: ToolOutputRecord,
            size_bytes: u64,
        }

        let mut items: Vec<ListEntry> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let record: ToolOutputRecord = match serde_json::from_str(&content) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Apply filters
            if let Some(cid) = filter_conversation_id {
                match &record.conversation_id {
                    Some(actual) if actual == cid => {}
                    _ => continue,
                }
            }
            if let Some(tn) = filter_tool_name {
                if record.tool_name != tn {
                    continue;
                }
            }
            if let Some(s) = filter_success {
                if record.success != s {
                    continue;
                }
            }
            if let Some(after) = filter_after {
                if record.created_at <= after {
                    continue;
                }
            }
            if let Some(before) = filter_before {
                if record.created_at >= before {
                    continue;
                }
            }

            items.push(ListEntry { record, size_bytes });
        }

        // Sort
        items.sort_by(|a, b| {
            let cmp = match sort_by {
                "size" => a.size_bytes.cmp(&b.size_bytes),
                "tool_name" => a.record.tool_name.cmp(&b.record.tool_name),
                _ => a.record.created_at.cmp(&b.record.created_at),
            };
            if sort_order == "desc" {
                cmp.reverse()
            } else {
                cmp
            }
        });

        let total = items.len();
        let has_more = offset + limit < total;

        // Paginate
        let page: Vec<&ListEntry> = items.iter().skip(offset).take(limit).collect();

        // Build output entries
        let outputs: Vec<Value> = page
            .iter()
            .map(|entry| {
                let mut obj = json!({
                    "id": entry.record.id,
                    "tool_name": entry.record.tool_name,
                    "conversation_id": entry.record.conversation_id,
                    "message_id": entry.record.message_id,
                    "created_at": entry.record.created_at,
                    "success": entry.record.success,
                    "size_bytes": entry.size_bytes,
                });

                // Summary
                let summary = match &entry.record.output {
                    Value::Object(map) => json!({
                        "type": "object",
                        "keys": map.len()
                    }),
                    Value::Array(arr) => json!({
                        "type": "array",
                        "items": arr.len()
                    }),
                    Value::String(_) => json!({ "type": "string" }),
                    Value::Number(_) => json!({ "type": "number" }),
                    Value::Bool(_) => json!({ "type": "boolean" }),
                    Value::Null => json!({ "type": "null" }),
                };
                obj["summary"] = summary;

                if include_preview {
                    let output_str =
                        serde_json::to_string(&entry.record.output).unwrap_or_default();
                    let preview: String = output_str.chars().take(preview_length).collect();
                    obj["preview"] = Value::String(preview);
                }

                obj
            })
            .collect();

        Ok(json!({
            "outputs": outputs,
            "total": total,
            "has_more": has_more
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
