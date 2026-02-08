use crate::db::Db;
use crate::tool_outputs::{read_tool_output, tool_outputs_root, ToolOutputRecord};
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde_json::{json, Value};
use serde_json_path::JsonPath;
use std::sync::Arc;

pub fn register_tool_output_tools(registry: &mut ToolRegistry, _db: Db) -> Result<(), String> {
    register_read_tool(registry)?;
    register_list_tool(registry)?;
    register_stats_tool(registry)?;
    register_extract_tool(registry)?;
    register_count_tool(registry)?;
    register_sample_tool(registry)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// tool_outputs.read (existing)
// ---------------------------------------------------------------------------

fn register_read_tool(registry: &mut ToolRegistry) -> Result<(), String> {
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

// ---------------------------------------------------------------------------
// tool_outputs.list
// ---------------------------------------------------------------------------

fn register_list_tool(registry: &mut ToolRegistry) -> Result<(), String> {
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
        let offset = args
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
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
        let entries = std::fs::read_dir(&root).map_err(|e| {
            ToolError::new(format!("Failed to read tool outputs directory: {e}"))
        })?;

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
                    let output_str = serde_json::to_string(&entry.record.output)
                        .unwrap_or_default();
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

// ---------------------------------------------------------------------------
// tool_outputs.stats
// ---------------------------------------------------------------------------

fn register_stats_tool(registry: &mut ToolRegistry) -> Result<(), String> {
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
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
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
        let output_str =
            serde_json::to_string(&record.output).unwrap_or_default();
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

// ---------------------------------------------------------------------------
// tool_outputs.extract
// ---------------------------------------------------------------------------

fn register_extract_tool(registry: &mut ToolRegistry) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "tool_outputs.extract".to_string(),
        description:
            "Extract specific fields from stored tool output using JSONPath expressions. Supports multiple paths and various output formats."
                .to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The tool output reference ID"
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Array of JSONPath expressions to extract",
                    "minItems": 1
                },
                "flatten": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to flatten results into a single array"
                },
                "include_paths": {
                    "type": "boolean",
                    "default": false,
                    "description": "Include the JSONPath expression with each result"
                },
                "default_value": {
                    "description": "Default value for missing paths (null if not specified)"
                }
            },
            "required": ["id", "paths"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "extracted": {
                    "description": "Extracted values, structure depends on flatten/include_paths options"
                },
                "missing_paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Paths that didn't match any values"
                }
            },
            "required": ["extracted"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Inline,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            return Err(ToolError::new("Missing 'id'"));
        }

        let paths = args
            .get("paths")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::new("Missing 'paths' array"))?;
        if paths.is_empty() {
            return Err(ToolError::new("'paths' array must not be empty"));
        }

        let flatten = args
            .get("flatten")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let include_paths = args
            .get("include_paths")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let default_value = args.get("default_value");

        let record = read_tool_output(id).map_err(ToolError::new)?;
        let mut missing_paths: Vec<String> = Vec::new();

        if flatten {
            // Flatten all results into a single array
            let mut all_values: Vec<Value> = Vec::new();
            for path_val in paths {
                let path_str = path_val
                    .as_str()
                    .ok_or_else(|| ToolError::new("Each path must be a string"))?;
                let jp = JsonPath::parse(path_str).map_err(|e| {
                    ToolError::new(format!("Invalid JSONPath '{path_str}': {e}"))
                })?;
                let nodes = jp.query(&record.output);
                let results: Vec<&Value> = nodes.all();
                if results.is_empty() {
                    missing_paths.push(path_str.to_string());
                    if let Some(dv) = default_value {
                        all_values.push(dv.clone());
                    }
                } else {
                    for node in results {
                        all_values.push(node.clone());
                    }
                }
            }
            let mut result = json!({ "extracted": all_values });
            if !missing_paths.is_empty() {
                result["missing_paths"] = json!(missing_paths);
            }
            Ok(result)
        } else if include_paths {
            // Return array of {path, value} objects
            let mut extracted: Vec<Value> = Vec::new();
            for path_val in paths {
                let path_str = path_val
                    .as_str()
                    .ok_or_else(|| ToolError::new("Each path must be a string"))?;
                let jp = JsonPath::parse(path_str).map_err(|e| {
                    ToolError::new(format!("Invalid JSONPath '{path_str}': {e}"))
                })?;
                let nodes = jp.query(&record.output);
                let results: Vec<&Value> = nodes.all();
                if results.is_empty() {
                    missing_paths.push(path_str.to_string());
                    let value = default_value.cloned().unwrap_or(Value::Null);
                    extracted.push(json!({ "path": path_str, "value": value }));
                } else {
                    let values: Vec<Value> = results.into_iter().cloned().collect();
                    extracted.push(json!({ "path": path_str, "value": values }));
                }
            }
            let mut result = json!({ "extracted": extracted });
            if !missing_paths.is_empty() {
                result["missing_paths"] = json!(missing_paths);
            }
            Ok(result)
        } else {
            // Default: object keyed by path expression
            let mut extracted = serde_json::Map::new();
            for path_val in paths {
                let path_str = path_val
                    .as_str()
                    .ok_or_else(|| ToolError::new("Each path must be a string"))?;
                let jp = JsonPath::parse(path_str).map_err(|e| {
                    ToolError::new(format!("Invalid JSONPath '{path_str}': {e}"))
                })?;
                let nodes = jp.query(&record.output);
                let results: Vec<&Value> = nodes.all();
                if results.is_empty() {
                    missing_paths.push(path_str.to_string());
                    let value = default_value.cloned().unwrap_or(Value::Null);
                    extracted.insert(path_str.to_string(), value);
                } else {
                    let values: Vec<Value> = results.into_iter().cloned().collect();
                    extracted.insert(path_str.to_string(), Value::Array(values));
                }
            }
            let mut result = json!({ "extracted": Value::Object(extracted) });
            if !missing_paths.is_empty() {
                result["missing_paths"] = json!(missing_paths);
            }
            Ok(result)
        }
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

// ---------------------------------------------------------------------------
// tool_outputs.count
// ---------------------------------------------------------------------------

fn register_count_tool(registry: &mut ToolRegistry) -> Result<(), String> {
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
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
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

            let jp = JsonPath::parse(path_str).map_err(|e| {
                ToolError::new(format!("Invalid JSONPath '{path_str}': {e}"))
            })?;
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
                "object_keys" => {
                    results
                        .iter()
                        .map(|v| match v {
                            Value::Object(map) => map.len() as i64,
                            _ => 0,
                        })
                        .sum()
                }
                "matches" => {
                    // Count the number of matched nodes
                    results.len() as i64
                }
                "nested_total" => {
                    // For each matched node, if it's an array, count all items recursively
                    results
                        .iter()
                        .map(|v| count_nested_items(v))
                        .sum()
                }
                _ => {
                    return Err(ToolError::new(format!(
                        "Unknown count_type '{count_type}'"
                    )));
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

/// Recursively count all items in nested arrays.
fn count_nested_items(value: &Value) -> i64 {
    match value {
        Value::Array(arr) => {
            let mut count = arr.len() as i64;
            for item in arr {
                if let Value::Array(_) = item {
                    count += count_nested_items(item);
                }
            }
            count
        }
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// tool_outputs.sample
// ---------------------------------------------------------------------------

fn register_sample_tool(registry: &mut ToolRegistry) -> Result<(), String> {
    let metadata = ToolMetadata {
        name: "tool_outputs.sample".to_string(),
        description:
            "Extract a sample of items from arrays in stored output. Supports random, systematic, and edge sampling strategies."
                .to_string(),
        args_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The tool output reference ID"
                },
                "path": {
                    "type": "string",
                    "description": "JSONPath to the array to sample from"
                },
                "size": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "Number of items to sample"
                },
                "strategy": {
                    "type": "string",
                    "enum": ["random", "first", "last", "systematic"],
                    "default": "random",
                    "description": "Sampling strategy to use"
                },
                "seed": {
                    "type": "integer",
                    "description": "Random seed for reproducible sampling"
                },
                "stride": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Step size for systematic sampling"
                }
            },
            "required": ["id", "path", "size"],
            "additionalProperties": false
        }),
        result_schema: json!({
            "type": "object",
            "properties": {
                "sample": {
                    "type": "array",
                    "description": "Sampled items"
                },
                "total_items": {
                    "type": "integer",
                    "description": "Total number of items in source array"
                },
                "sample_size": {
                    "type": "integer",
                    "description": "Actual number of items sampled"
                },
                "indices": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Indices of sampled items in original array"
                }
            },
            "required": ["sample", "total_items", "sample_size"],
            "additionalProperties": false
        }),
        requires_approval: false,
        result_mode: ToolResultMode::Auto,
    };

    let handler = Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            return Err(ToolError::new("Missing 'id'"));
        }

        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing 'path'"))?;
        let size = args
            .get("size")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ToolError::new("Missing 'size'"))? as usize;
        let strategy = args
            .get("strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("random");
        let seed = args.get("seed").and_then(|v| v.as_u64());
        let stride = args
            .get("stride")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;

        let record = read_tool_output(id).map_err(ToolError::new)?;

        let jp = JsonPath::parse(path_str).map_err(|e| {
            ToolError::new(format!("Invalid JSONPath '{path_str}': {e}"))
        })?;
        let nodes = jp.query(&record.output);
        let results: Vec<&Value> = nodes.all();

        // Find the first array result
        let arr = results
            .iter()
            .find_map(|v| v.as_array())
            .ok_or_else(|| {
                ToolError::new(format!(
                    "Path '{path_str}' did not match an array"
                ))
            })?;

        let total_items = arr.len();
        let actual_size = size.min(total_items);

        let (sampled_indices, sample): (Vec<usize>, Vec<Value>) = match strategy {
            "first" => {
                let indices: Vec<usize> = (0..actual_size).collect();
                let items: Vec<Value> = indices.iter().map(|&i| arr[i].clone()).collect();
                (indices, items)
            }
            "last" => {
                let start = total_items.saturating_sub(actual_size);
                let indices: Vec<usize> = (start..total_items).collect();
                let items: Vec<Value> = indices.iter().map(|&i| arr[i].clone()).collect();
                (indices, items)
            }
            "systematic" => {
                let mut indices: Vec<usize> = Vec::new();
                let mut i = 0;
                while indices.len() < actual_size && i < total_items {
                    indices.push(i);
                    i += stride.max(1);
                }
                let items: Vec<Value> = indices.iter().map(|&i| arr[i].clone()).collect();
                (indices, items)
            }
            _ => {
                // "random" (default)
                let mut index_pool: Vec<usize> = (0..total_items).collect();
                let mut rng: StdRng = match seed {
                    Some(s) => StdRng::seed_from_u64(s),
                    None => StdRng::from_entropy(),
                };
                index_pool.shuffle(&mut rng);
                let mut indices: Vec<usize> =
                    index_pool.into_iter().take(actual_size).collect();
                indices.sort_unstable();
                let items: Vec<Value> = indices.iter().map(|&i| arr[i].clone()).collect();
                (indices, items)
            }
        };

        Ok(json!({
            "sample": sample,
            "total_items": total_items,
            "sample_size": sampled_indices.len(),
            "indices": sampled_indices
        }))
    });

    registry.register(ToolDefinition {
        metadata,
        handler,
        preview: None,
    })
}

// ---------------------------------------------------------------------------
// Stats helpers
// ---------------------------------------------------------------------------

#[derive(Default)]
struct TypeCounts {
    objects: u64,
    arrays: u64,
    strings: u64,
    numbers: u64,
    booleans: u64,
    nulls: u64,
}

#[derive(Default)]
struct JsonStats {
    max_depth: usize,
    total_keys: u64,
    total_values: u64,
    type_counts: TypeCounts,
}

impl JsonStats {
    fn merge(&mut self, other: &JsonStats) {
        if other.max_depth > self.max_depth {
            self.max_depth = other.max_depth;
        }
        self.total_keys += other.total_keys;
        self.total_values += other.total_values;
        self.type_counts.objects += other.type_counts.objects;
        self.type_counts.arrays += other.type_counts.arrays;
        self.type_counts.strings += other.type_counts.strings;
        self.type_counts.numbers += other.type_counts.numbers;
        self.type_counts.booleans += other.type_counts.booleans;
        self.type_counts.nulls += other.type_counts.nulls;
    }
}

fn walk_value(
    value: &Value,
    path: &str,
    depth: usize,
    max_depth: usize,
    sample_arrays: bool,
    stats: &mut JsonStats,
    arrays: &mut Vec<Value>,
    objects: &mut Vec<Value>,
) {
    if depth > stats.max_depth {
        stats.max_depth = depth;
    }

    stats.total_values += 1;

    match value {
        Value::Object(map) => {
            stats.type_counts.objects += 1;
            stats.total_keys += map.len() as u64;
            objects.push(json!({ "path": path, "keys": map.len() }));

            if depth < max_depth {
                for (key, val) in map {
                    let child_path = format!("{path}.{key}");
                    walk_value(val, &child_path, depth + 1, max_depth, sample_arrays, stats, arrays, objects);
                }
            }
        }
        Value::Array(arr) => {
            stats.type_counts.arrays += 1;
            let item_type = if sample_arrays && !arr.is_empty() {
                determine_array_item_type(arr)
            } else {
                "unknown".to_string()
            };
            arrays.push(json!({
                "path": path,
                "length": arr.len(),
                "item_type": item_type
            }));

            if depth < max_depth {
                // Walk a sample of array items to gather stats (first, middle, last)
                let indices = sample_indices(arr.len());
                for idx in indices {
                    let child_path = format!("{path}[{idx}]");
                    walk_value(&arr[idx], &child_path, depth + 1, max_depth, sample_arrays, stats, arrays, objects);
                }
            }
        }
        Value::String(_) => {
            stats.type_counts.strings += 1;
        }
        Value::Number(_) => {
            stats.type_counts.numbers += 1;
        }
        Value::Bool(_) => {
            stats.type_counts.booleans += 1;
        }
        Value::Null => {
            stats.type_counts.nulls += 1;
        }
    }
}

/// Pick representative indices from an array: first, middle, last (deduplicated).
fn sample_indices(len: usize) -> Vec<usize> {
    if len == 0 {
        return vec![];
    }
    let mut indices = vec![0];
    if len > 2 {
        indices.push(len / 2);
    }
    if len > 1 {
        indices.push(len - 1);
    }
    indices.sort_unstable();
    indices.dedup();
    indices
}

fn determine_array_item_type(arr: &[Value]) -> String {
    if arr.is_empty() {
        return "unknown".to_string();
    }
    let first_type = json_type_name(&arr[0]);
    let all_same = arr.iter().take(10).all(|v| json_type_name(v) == first_type);
    if all_same {
        first_type.to_string()
    } else {
        "mixed".to_string()
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Infer a JSON Schema from a value, up to a depth limit.
fn infer_schema(value: &Value, depth: usize, max_depth: usize, sample_arrays: bool) -> Value {
    if depth >= max_depth {
        return json!({});
    }

    match value {
        Value::Object(map) => {
            let mut properties = serde_json::Map::new();
            for (key, val) in map {
                properties.insert(
                    key.clone(),
                    infer_schema(val, depth + 1, max_depth, sample_arrays),
                );
            }
            json!({
                "type": "object",
                "properties": Value::Object(properties)
            })
        }
        Value::Array(arr) => {
            let items_schema = if sample_arrays && !arr.is_empty() {
                infer_schema(&arr[0], depth + 1, max_depth, sample_arrays)
            } else {
                json!({})
            };
            json!({
                "type": "array",
                "items": items_schema
            })
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Number(_) => json!({ "type": "number" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Null => json!({ "type": "null" }),
    }
}
