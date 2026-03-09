use super::{format_bytes, infer_schema, json_type_name, walk_value, JsonStats};
use crate::tool_outputs::read_tool_output;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn register_stats_tool(registry: &mut ToolRegistry) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "tool_outputs.stats".to_string(),
        description:
            "Get statistics and metadata about stored tool output including size, structure, types, and optional schema generation."
                .to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The tool output reference ID"
                },
                "include_schema": {
                    "type": "boolean",
                    "default": false,
                    "description": "Generate and include JSON schema of the data"
                },
                "max_depth": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "default": 5,
                    "description": "Maximum depth to analyze"
                },
                "sample_arrays": {
                    "type": "boolean",
                    "default": true,
                    "description": "Sample arrays to determine item types"
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Specific paths to analyze (analyzes root if not specified)"
                }
            },
            "required": ["id"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "tool_name": { "type": "string" },
                "created_at": { "type": "integer" },
                "size": {
                    "type": "object",
                    "properties": {
                        "bytes": { "type": "integer" },
                        "characters": { "type": "integer" },
                        "formatted": { "type": "string" }
                    }
                },
                "structure": {
                    "type": "object",
                    "properties": {
                        "root_type": { "type": "string" },
                        "max_depth": { "type": "integer" },
                        "total_keys": { "type": "integer" },
                        "total_values": { "type": "integer" }
                    }
                },
                "types": {
                    "type": "object",
                    "additionalProperties": { "type": "integer" }
                },
                "arrays": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "length": { "type": "integer" },
                            "item_type": { "type": "string" }
                        }
                    }
                },
                "objects": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "keys": { "type": "integer" }
                        }
                    }
                },
                "schema": {
                    "description": "Generated JSON schema if requested"
                }
            },
            "required": ["id", "size", "structure", "types"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
        if id.is_empty() {
            return Err(ToolError::new("Missing 'id'"));
        }

        let include_schema = args
            .get("include_schema")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_depth = args
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(10) as usize;
        let sample_arrays = args
            .get("sample_arrays")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let record = read_tool_output(id).map_err(ToolError::new)?;

        // Determine which value(s) to analyze
        let targets: Vec<(&str, &Value)> =
            if let Some(paths_arr) = args.get("paths").and_then(|v| v.as_array()) {
                let mut targets = Vec::new();
                for path_val in paths_arr {
                    if let Some(path_str) = path_val.as_str() {
                        let jp = serde_json_path::JsonPath::parse(path_str).map_err(|e| {
                            ToolError::new(format!("Invalid JSONPath '{path_str}': {e}"))
                        })?;
                        let nodes = jp.query(&record.output);
                        for node in nodes.all() {
                            targets.push((path_str, node));
                        }
                    }
                }
                targets
            } else {
                vec![("$", &record.output)]
            };

        // Accumulate stats across all targets
        let mut total_stats = JsonStats::default();
        let mut all_arrays: Vec<Value> = Vec::new();
        let mut all_objects: Vec<Value> = Vec::new();

        for (path_prefix, value) in &targets {
            let mut stats = JsonStats::default();
            walk_value(
                value,
                path_prefix,
                0,
                max_depth,
                sample_arrays,
                &mut stats,
                &mut all_arrays,
                &mut all_objects,
            );
            total_stats.merge(&stats);
        }

        // Compute serialized size
        let output_str = serde_json::to_string(&record.output).unwrap_or_default();
        let bytes = output_str.len() as u64;

        let mut result = json!({
            "id": record.id,
            "tool_name": record.tool_name,
            "created_at": record.created_at,
            "size": {
                "bytes": bytes,
                "characters": output_str.len(),
                "formatted": format_bytes(bytes)
            },
            "structure": {
                "root_type": json_type_name(&record.output),
                "max_depth": total_stats.max_depth,
                "total_keys": total_stats.total_keys,
                "total_values": total_stats.total_values
            },
            "types": {
                "object": total_stats.type_counts.objects,
                "array": total_stats.type_counts.arrays,
                "string": total_stats.type_counts.strings,
                "number": total_stats.type_counts.numbers,
                "boolean": total_stats.type_counts.booleans,
                "null": total_stats.type_counts.nulls
            },
            "arrays": all_arrays,
            "objects": all_objects
        });

        if include_schema {
            let schema = infer_schema(&record.output, 0, max_depth, sample_arrays);
            result["schema"] = schema;
        }

        Ok(result)
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
