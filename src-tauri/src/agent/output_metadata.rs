use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::text_utils::{truncate_with_notice, value_char_len};

const OUTPUT_METADATA_MAX_TOP_LEVEL_KEYS: usize = 20;
const OUTPUT_METADATA_MAX_ID_HINTS: usize = 12;
const OUTPUT_METADATA_MAX_ID_SAMPLE_CHARS: usize = 80;
const OUTPUT_METADATA_MAX_ITEM_TYPE_HINTS: usize = 8;
const OUTPUT_METADATA_SCAN_MAX_DEPTH: usize = 4;
const OUTPUT_METADATA_SCAN_MAX_ARRAY_ITEMS: usize = 24;
const OUTPUT_METADATA_MAX_SERIALIZED_CHARS: usize = 1_600;

pub fn compute_output_metadata(value: &Value) -> Value {
    compute_output_metadata_with_size(value, None)
}

pub fn compute_output_metadata_with_size(value: &Value, precomputed_size: Option<usize>) -> Value {
    let size_chars = precomputed_size.unwrap_or_else(|| value_char_len(value));
    let mut metadata = match value {
        Value::Object(map) => {
            let mut sorted_keys = map.keys().cloned().collect::<Vec<_>>();
            sorted_keys.sort();

            let top_level_keys = sorted_keys
                .iter()
                .take(OUTPUT_METADATA_MAX_TOP_LEVEL_KEYS)
                .cloned()
                .collect::<Vec<_>>();
            let top_level_value_types = top_level_keys
                .iter()
                .take(OUTPUT_METADATA_MAX_ITEM_TYPE_HINTS)
                .filter_map(|key| {
                    map.get(key).map(|entry| {
                        json!({
                            "key": key,
                            "type": json_type_name(entry)
                        })
                    })
                })
                .collect::<Vec<_>>();

            json!({
                "root_type": "object",
                "size_chars": size_chars,
                "key_count": map.len(),
                "top_level_keys": top_level_keys,
                "top_level_value_types": top_level_value_types
            })
        }
        Value::Array(arr) => json!({
            "root_type": "array",
            "size_chars": size_chars,
            "array_length": arr.len(),
            "item_type_hints": array_item_type_hints(arr)
        }),
        Value::String(text) => json!({
            "root_type": "string",
            "size_chars": size_chars,
            "string_length": text.chars().count()
        }),
        Value::Number(_) => json!({
            "root_type": "number",
            "size_chars": size_chars
        }),
        Value::Bool(_) => json!({
            "root_type": "boolean",
            "size_chars": size_chars
        }),
        Value::Null => json!({
            "root_type": "null",
            "size_chars": size_chars
        }),
    };

    let mut id_hints = Vec::new();
    collect_id_like_hints(value, "$", 0, &mut id_hints);
    if !id_hints.is_empty() {
        if let Some(object) = metadata.as_object_mut() {
            object.insert("id_hints".to_string(), Value::Array(id_hints));
        }
    }

    bound_output_metadata_size(metadata)
}

pub fn strip_metadata_id_hints(metadata: &Value) -> Value {
    match metadata {
        Value::Object(map) => {
            let mut cleaned = map.clone();
            cleaned.remove("id_hints");
            Value::Object(cleaned)
        }
        other => other.clone(),
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

fn array_item_type_hints(values: &[Value]) -> Value {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for item in values.iter().take(OUTPUT_METADATA_SCAN_MAX_ARRAY_ITEMS) {
        let entry = counts.entry(json_type_name(item)).or_insert(0);
        *entry += 1;
    }

    let hints = counts
        .into_iter()
        .take(OUTPUT_METADATA_MAX_ITEM_TYPE_HINTS)
        .map(|(value_type, count)| {
            json!({
                "type": value_type,
                "count": count
            })
        })
        .collect::<Vec<_>>();

    Value::Array(hints)
}

fn collect_id_like_hints(value: &Value, path: &str, depth: usize, hints: &mut Vec<Value>) {
    if depth > OUTPUT_METADATA_SCAN_MAX_DEPTH || hints.len() >= OUTPUT_METADATA_MAX_ID_HINTS {
        return;
    }

    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();

            for key in keys {
                if hints.len() >= OUTPUT_METADATA_MAX_ID_HINTS {
                    break;
                }
                let Some(child) = map.get(key) else {
                    continue;
                };
                let child_path = format!("{path}.{key}");

                if is_id_like_key(key) {
                    let mut hint = serde_json::Map::new();
                    hint.insert("path".to_string(), Value::String(child_path.clone()));
                    hint.insert("key".to_string(), Value::String(key.clone()));
                    hint.insert(
                        "value_type".to_string(),
                        Value::String(json_type_name(child).to_string()),
                    );
                    if let Some(sample) = summarize_id_sample(child) {
                        hint.insert("sample".to_string(), Value::String(sample));
                    }
                    hints.push(Value::Object(hint));
                }

                collect_id_like_hints(child, &child_path, depth + 1, hints);
            }
        }
        Value::Array(array) => {
            for (index, child) in array
                .iter()
                .take(OUTPUT_METADATA_SCAN_MAX_ARRAY_ITEMS)
                .enumerate()
            {
                if hints.len() >= OUTPUT_METADATA_MAX_ID_HINTS {
                    break;
                }
                let child_path = format!("{path}[{index}]");
                collect_id_like_hints(child, &child_path, depth + 1, hints);
            }
        }
        _ => {}
    }
}

fn is_id_like_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    normalized == "id" || normalized.ends_with("_id") || normalized.ends_with("id")
}

fn summarize_id_sample(value: &Value) -> Option<String> {
    let raw = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => return None,
    };

    Some(truncate_with_notice(&raw, OUTPUT_METADATA_MAX_ID_SAMPLE_CHARS))
}

fn bound_output_metadata_size(mut metadata: Value) -> Value {
    let mut length = value_char_len(&metadata);
    if length <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS {
        return metadata;
    }

    // Priority: keep id_hints (most useful for the LLM), drop decorative metadata first
    if let Some(object) = metadata.as_object_mut() {
        object.remove("top_level_value_types");
        object.remove("item_type_hints");
        object.insert("metadata_truncated".to_string(), Value::Bool(true));
    }

    length = value_char_len(&metadata);
    if length <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS {
        return metadata;
    }

    // Still too large — trim top_level_keys
    if let Some(object) = metadata.as_object_mut() {
        object.remove("top_level_keys");
    }

    length = value_char_len(&metadata);
    if length <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS {
        return metadata;
    }

    // Last resort — drop id_hints too
    if let Some(object) = metadata.as_object_mut() {
        object.remove("id_hints");
    }

    length = value_char_len(&metadata);
    if length <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS {
        return metadata;
    }

    json!({
        "root_type": metadata.get("root_type").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "size_chars": metadata.get("size_chars").cloned().unwrap_or_else(|| Value::from(0)),
        "metadata_truncated": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compute_output_metadata_for_object() {
        let value = json!({ "name": "test", "count": 42, "items": [1, 2, 3] });
        let metadata = compute_output_metadata(&value);
        assert_eq!(
            metadata.get("root_type").and_then(|v| v.as_str()),
            Some("object")
        );
        assert_eq!(
            metadata.get("key_count").and_then(|v| v.as_u64()),
            Some(3)
        );
        let keys = metadata
            .get("top_level_keys")
            .and_then(|v| v.as_array())
            .expect("top_level_keys");
        assert!(keys.len() == 3);
    }

    #[test]
    fn compute_output_metadata_for_array() {
        let value = json!([1, 2, 3, 4, 5]);
        let metadata = compute_output_metadata(&value);
        assert_eq!(
            metadata.get("root_type").and_then(|v| v.as_str()),
            Some("array")
        );
        assert_eq!(
            metadata.get("array_length").and_then(|v| v.as_u64()),
            Some(5)
        );
    }

    #[test]
    fn compute_output_metadata_for_string() {
        let value = json!("hello world");
        let metadata = compute_output_metadata(&value);
        assert_eq!(
            metadata.get("root_type").and_then(|v| v.as_str()),
            Some("string")
        );
        assert_eq!(
            metadata.get("string_length").and_then(|v| v.as_u64()),
            Some(11)
        );
    }

    #[test]
    fn compute_output_metadata_includes_rich_bounded_hints() {
        let value = json!({
            "threads": [
                { "id": "thread-1", "message_id": "msg-1" },
                { "id": "thread-2", "message_id": "msg-2" }
            ]
        });
        let metadata = compute_output_metadata(&value);
        let id_hints = metadata
            .get("id_hints")
            .and_then(|v| v.as_array())
            .expect("id_hints");
        assert!(
            !id_hints.is_empty(),
            "should have id hints for id-like fields"
        );
        let paths: Vec<&str> = id_hints
            .iter()
            .filter_map(|h| h.get("key").and_then(|k| k.as_str()))
            .collect();
        assert!(paths.contains(&"id"), "should detect 'id' key");
        assert!(paths.contains(&"message_id"), "should detect 'message_id' key");
    }

    #[test]
    fn compute_output_metadata_stays_under_size_limit() {
        let entries: Vec<Value> = (0..256)
            .map(|i| {
                json!({
                    "id": format!("entry-{i}"),
                    "thread_id": format!("thread-{i}"),
                    "message_id": format!("msg-{i}"),
                    "data": "x".repeat(100)
                })
            })
            .collect();
        let value = json!(entries);
        let metadata = compute_output_metadata(&value);
        let serialized = serde_json::to_string(&metadata).unwrap();
        assert!(
            serialized.chars().count() <= OUTPUT_METADATA_MAX_SERIALIZED_CHARS,
            "metadata serialized size {} exceeds limit {}",
            serialized.chars().count(),
            OUTPUT_METADATA_MAX_SERIALIZED_CHARS
        );
    }
}
