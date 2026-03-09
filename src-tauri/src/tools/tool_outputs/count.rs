use super::count_nested_items;
use crate::tool_outputs::read_tool_output;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use serde_json_path::JsonPath;
use std::sync::Arc;

pub(super) fn register_count_tool(registry: &mut ToolRegistry) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "tool_outputs.count".to_string(),
        description:
            "Count items in arrays, object keys, or matches without loading full data. Efficient for large datasets."
                .to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The tool output reference ID"
                },
                "counts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Name for this count operation"
                            },
                            "path": {
                                "type": "string",
                                "description": "JSONPath to the element to count"
                            },
                            "filter": {
                                "type": "string",
                                "description": "Optional JSONPath filter expression"
                            },
                            "count_type": {
                                "type": "string",
                                "enum": ["array_length", "object_keys", "matches", "nested_total"],
                                "default": "array_length",
                                "description": "Type of counting operation"
                            }
                        },
                        "required": ["name", "path"],
                        "additionalProperties": false
                    },
                    "minItems": 1,
                    "description": "Array of count operations to perform"
                }
            },
            "required": ["id", "counts"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "counts": {
                    "type": "object",
                    "additionalProperties": { "type": "integer" }
                },
                "total": {
                    "type": "integer",
                    "description": "Sum of all counts"
                }
            },
            "required": ["counts"],
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

        let count_ops = args
            .get("counts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::new("Missing 'counts' array"))?;

        let record = read_tool_output(id).map_err(ToolError::new)?;

        let mut counts = serde_json::Map::new();
        let mut total: i64 = 0;

        for op in count_ops {
            let name = op
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::new("Each count operation requires 'name'"))?;
            let path_str = op
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::new("Each count operation requires 'path'"))?;
            let count_type = op
                .get("count_type")
                .and_then(|v| v.as_str())
                .unwrap_or("array_length");

            let jp = JsonPath::parse(path_str)
                .map_err(|e| ToolError::new(format!("Invalid JSONPath '{path_str}': {e}")))?;
            let nodes = jp.query(&record.output);
            let results: Vec<&Value> = nodes.all();

            let count: i64 = match count_type {
                "array_length" => {
                    // If the path points to an array, return its length
                    // If multiple matches, sum all array lengths
                    results
                        .iter()
                        .map(|v| match v {
                            Value::Array(arr) => arr.len() as i64,
                            _ => 0,
                        })
                        .sum()
                }
                "object_keys" => results
                    .iter()
                    .map(|v| match v {
                        Value::Object(map) => map.len() as i64,
                        _ => 0,
                    })
                    .sum(),
                "matches" => {
                    // Count the number of matched nodes
                    results.len() as i64
                }
                "nested_total" => {
                    // For each matched node, if it's an array, count all items recursively
                    results.iter().map(|v| count_nested_items(v)).sum()
                }
                _ => {
                    return Err(ToolError::new(format!("Unknown count_type '{count_type}'")));
                }
            };

            total += count;
            counts.insert(name.to_string(), json!(count));
        }

        Ok(json!({
            "counts": Value::Object(counts),
            "total": total
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}
