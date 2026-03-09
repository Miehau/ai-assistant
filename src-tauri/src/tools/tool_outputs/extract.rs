use crate::tool_outputs::read_tool_output;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use serde_json::{json, Value};
use serde_json_path::JsonPath;
use std::sync::Arc;

pub(super) fn register_extract_tool(registry: &mut ToolRegistry) -> Result<(), String> {
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
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
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
                let jp = JsonPath::parse(path_str)
                    .map_err(|e| ToolError::new(format!("Invalid JSONPath '{path_str}': {e}")))?;
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
                let jp = JsonPath::parse(path_str)
                    .map_err(|e| ToolError::new(format!("Invalid JSONPath '{path_str}': {e}")))?;
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
                let jp = JsonPath::parse(path_str)
                    .map_err(|e| ToolError::new(format!("Invalid JSONPath '{path_str}': {e}")))?;
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
