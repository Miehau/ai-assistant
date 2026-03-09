mod count;
mod extract;
mod list;
mod read;
mod sample;
mod stats;

use crate::db::Db;
use crate::tools::ToolRegistry;
use serde_json::{json, Value};

pub fn register_tool_output_tools(registry: &mut ToolRegistry, _db: Db) -> Result<(), String> {
    read::register_read_tool(registry)?;
    list::register_list_tool(registry)?;
    stats::register_stats_tool(registry)?;
    extract::register_extract_tool(registry)?;
    count::register_count_tool(registry)?;
    sample::register_sample_tool(registry)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Stats helpers
// ---------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct TypeCounts {
    pub(crate) objects: u64,
    pub(crate) arrays: u64,
    pub(crate) strings: u64,
    pub(crate) numbers: u64,
    pub(crate) booleans: u64,
    pub(crate) nulls: u64,
}

#[derive(Default)]
pub(crate) struct JsonStats {
    pub(crate) max_depth: usize,
    pub(crate) total_keys: u64,
    pub(crate) total_values: u64,
    pub(crate) type_counts: TypeCounts,
}

impl JsonStats {
    pub(crate) fn merge(&mut self, other: &JsonStats) {
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

pub(crate) fn walk_value(
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
                    walk_value(
                        val,
                        &child_path,
                        depth + 1,
                        max_depth,
                        sample_arrays,
                        stats,
                        arrays,
                        objects,
                    );
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
                    walk_value(
                        &arr[idx],
                        &child_path,
                        depth + 1,
                        max_depth,
                        sample_arrays,
                        stats,
                        arrays,
                        objects,
                    );
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
pub(crate) fn sample_indices(len: usize) -> Vec<usize> {
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

pub(crate) fn determine_array_item_type(arr: &[Value]) -> String {
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

pub(crate) fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
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
pub(crate) fn infer_schema(value: &Value, depth: usize, max_depth: usize, sample_arrays: bool) -> Value {
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

/// Recursively count all items in nested arrays.
pub(crate) fn count_nested_items(value: &Value) -> i64 {
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
