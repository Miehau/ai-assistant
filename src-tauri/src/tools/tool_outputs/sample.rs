use crate::tool_outputs::read_tool_output;
use crate::tools::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolMetadata, ToolRegistry, ToolResultMode,
};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde_json::{json, Value};
use serde_json_path::JsonPath;
use std::sync::Arc;

pub(super) fn register_sample_tool(registry: &mut ToolRegistry) -> Result<(), String> {
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
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
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
        let stride = args.get("stride").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

        let record = read_tool_output(id).map_err(ToolError::new)?;

        let jp = JsonPath::parse(path_str)
            .map_err(|e| ToolError::new(format!("Invalid JSONPath '{path_str}': {e}")))?;
        let nodes = jp.query(&record.output);
        let results: Vec<&Value> = nodes.all();

        // Find the first array result
        let arr = results
            .iter()
            .find_map(|v| v.as_array())
            .ok_or_else(|| ToolError::new(format!("Path '{path_str}' did not match an array")))?;

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
                let mut indices: Vec<usize> = index_pool.into_iter().take(actual_size).collect();
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
