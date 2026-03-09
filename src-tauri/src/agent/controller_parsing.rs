use crate::db::ResumeTarget;
use crate::llm::json_schema_output_format;
use serde::Deserialize;
use serde_json::{json, Value};

const CONTROLLER_JSON_START_MARKER: &str = "=====JSON_START=====";
const CONTROLLER_JSON_END_MARKER: &str = "=====JSON_END=====";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputModeHint {
    Auto,
    Inline,
    Persist,
}

impl OutputModeHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Inline => "inline",
            Self::Persist => "persist",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "inline" => Some(Self::Inline),
            "persist" => Some(Self::Persist),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedOutputMode {
    Inline,
    Persist,
}

impl ResolvedOutputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Persist => "persist",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputDeliveryResolution {
    pub requested_output_mode: OutputModeHint,
    pub resolved_output_mode: ResolvedOutputMode,
    pub forced_persist: bool,
    pub forced_reason: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ControllerAction {
    NextStep {
        thinking: Value,
        #[serde(rename = "type")]
        step_type: Option<String>,
        description: Option<String>,
        tool: Option<String>,
        tools: Option<Vec<ControllerToolCallSpec>>,
        #[serde(default)]
        args: Value,
        output_mode: Option<String>,
        message: Option<String>,
        question: Option<String>,
        context: Option<String>,
        resume_to: Option<ResumeTarget>,
    },
    Complete {
        message: String,
    },
    GuardrailStop {
        reason: String,
        message: Option<String>,
    },
    AskUser {
        question: String,
        #[serde(default)]
        context: Option<String>,
        #[serde(default = "default_resume_target")]
        resume_to: ResumeTarget,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerToolCallSpec {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
    pub output_mode: Option<String>,
}

impl ControllerAction {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            ControllerAction::NextStep {
                step_type,
                tool,
                tools,
                message,
                question,
                output_mode,
                ..
            } => {
                let effective_type = step_type
                    .as_deref()
                    .or_else(|| infer_step_type_flat(tool, tools, message, question));
                match effective_type {
                    Some("tool") => {
                        if tool.as_ref().map_or(true, |t| t.trim().is_empty()) {
                            return Err(
                                "next_step type=tool requires non-empty 'tool' field".into()
                            );
                        }
                        if let Some(mode) = output_mode.as_deref() {
                            if OutputModeHint::parse(mode).is_none() {
                                return Err(format!(
                                    "Invalid output_mode '{mode}': expected one of auto, inline, persist"
                                ));
                            }
                        }
                    }
                    Some("tool_batch") => {
                        let entries = tools.as_ref().ok_or_else(|| {
                            "next_step type=tool_batch requires non-empty 'tools' field".to_string()
                        })?;
                        if entries.is_empty() {
                            return Err(
                                "next_step type=tool_batch requires non-empty 'tools' field"
                                    .to_string(),
                            );
                        }
                        for (idx, entry) in entries.iter().enumerate() {
                            if entry.tool.trim().is_empty() {
                                return Err(format!(
                                    "next_step type=tool_batch requires non-empty tool name at tools[{idx}]"
                                ));
                            }
                            if let Some(mode) = entry.output_mode.as_deref() {
                                if OutputModeHint::parse(mode).is_none() {
                                    return Err(format!(
                                        "Invalid output_mode '{mode}' at tools[{idx}]: expected one of auto, inline, persist"
                                    ));
                                }
                            }
                        }
                    }
                    Some("respond") => {
                        if message.as_ref().map_or(true, |m| m.trim().is_empty()) {
                            return Err(
                                "next_step type=respond requires non-empty 'message' field".into(),
                            );
                        }
                    }
                    Some("ask_user") => {
                        if question.as_ref().map_or(true, |q| q.trim().is_empty()) {
                            return Err(
                                "next_step type=ask_user requires non-empty 'question' field"
                                    .into(),
                            );
                        }
                    }
                    None => return Err(
                        "Cannot determine step type: provide 'type' or 'tool'/'message'/'question'"
                            .into(),
                    ),
                    Some(other) => return Err(format!("Unknown step type: {other}")),
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

pub fn infer_step_type_flat(
    tool: &Option<String>,
    tools: &Option<Vec<ControllerToolCallSpec>>,
    message: &Option<String>,
    question: &Option<String>,
) -> Option<&'static str> {
    if tools.as_ref().is_some_and(|entries| !entries.is_empty()) {
        return Some("tool_batch");
    }
    if tool.as_ref().is_some_and(|t| !t.trim().is_empty()) {
        return Some("tool");
    }
    if question.as_ref().is_some_and(|q| !q.trim().is_empty()) {
        return Some("ask_user");
    }
    if message.as_ref().is_some_and(|m| !m.trim().is_empty()) {
        return Some("respond");
    }
    None
}

pub fn default_resume_target() -> ResumeTarget {
    ResumeTarget::Reflecting
}

pub fn default_step_description(step_type: &str) -> &'static str {
    match step_type {
        "tool" => "Call the selected tool",
        "tool_batch" => "Execute a batch of tool calls",
        "respond" => "Respond to the user",
        "ask_user" => "Ask the user for clarification",
        _ => "Continue with the next step",
    }
}

pub fn parse_controller_action(value: &Value) -> Result<ControllerAction, String> {
    let normalized = normalize_controller_value(value);

    match serde_json::from_value::<ControllerAction>(normalized.clone()) {
        Ok(action) => {
            action.validate()?;
            Ok(action)
        }
        Err(serde_err) => {
            let action_str = normalized.get("action").and_then(|v| v.as_str());
            if action_str == Some("respond") {
                if let Some(msg) = non_empty_string_field(&normalized, &["message", "response"]) {
                    return Ok(ControllerAction::Complete { message: msg });
                }
            }

            Err(format!("Invalid controller output: {serde_err}"))
        }
    }
}

pub fn normalize_controller_value(value: &Value) -> Value {
    let Value::Object(map) = value else {
        return value.clone();
    };
    let mut out = map.clone();

    // Hoist nested step fields to top level
    if let Some(Value::Object(step)) = out.remove("step").or_else(|| out.remove("next_step")) {
        for (key, val) in step {
            out.entry(key).or_insert(val);
        }
    }

    // Normalize tool name aliases
    for alias in ["tool_name", "name"] {
        if out.get("tool").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("tool".to_string(), val);
            }
        }
    }

    // Normalize args aliases
    for alias in ["tool_args", "arguments", "tool_input"] {
        if out.get("args").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("args".to_string(), val);
            }
        }
    }

    // Normalize tool batch aliases
    for alias in ["tool_calls", "calls"] {
        if out.get("tools").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("tools".to_string(), val);
            }
        }
    }
    if let Some(Value::Array(entries)) = out.get_mut("tools") {
        for entry in entries {
            if let Value::Object(tool_entry) = entry {
                for alias in ["tool_name", "name"] {
                    if tool_entry.get("tool").is_none() {
                        if let Some(val) = tool_entry.remove(alias) {
                            tool_entry.insert("tool".to_string(), val);
                        }
                    }
                }
                for alias in ["tool_args", "arguments", "tool_input"] {
                    if tool_entry.get("args").is_none() {
                        if let Some(val) = tool_entry.remove(alias) {
                            tool_entry.insert("args".to_string(), val);
                        }
                    }
                }
            }
        }
    }

    // Normalize message aliases
    for alias in ["response", "content"] {
        if out.get("message").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("message".to_string(), val);
            }
        }
    }

    Value::Object(out)
}

pub fn extract_json(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(marked) = extract_marked_json(trimmed) {
        return marked;
    }
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or("");
    if !first_line.starts_with("```") {
        return trimmed.to_string();
    }

    let mut json_lines: Vec<&str> = lines.collect();
    if let Some(last) = json_lines.last() {
        if last.trim().starts_with("```") {
            json_lines.pop();
        }
    }

    json_lines.join("\n").trim().to_string()
}

fn extract_marked_json(raw: &str) -> Option<String> {
    let start = raw.find(CONTROLLER_JSON_START_MARKER)?;
    let after_start = start + CONTROLLER_JSON_START_MARKER.len();
    let end_relative = raw[after_start..].find(CONTROLLER_JSON_END_MARKER)?;
    let end = after_start + end_relative;
    Some(raw[after_start..end].trim().to_string())
}

fn non_empty_string_field(root: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        root.get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
    })
}

pub fn parse_output_mode_hint(value: Option<&str>) -> Result<OutputModeHint, String> {
    match value {
        None => Ok(OutputModeHint::Auto),
        Some(raw) => OutputModeHint::parse(raw).ok_or_else(|| {
            format!("Invalid output_mode '{raw}': expected one of auto, inline, persist")
        }),
    }
}

pub fn normalize_tool_args(args: Value) -> Value {
    match args {
        Value::Null => json!({}),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return json!({});
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(value) if value.is_object() => value,
                Ok(value) => json!({ "value": value }),
                Err(_) => json!({ "input": text }),
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_json_prefers_marked_envelope() {
        let raw = r#"some preamble
=====JSON_START=====
{"action":"complete","message":"done"}
=====JSON_END=====
trailing text"#;
        let result = extract_json(raw);
        assert_eq!(result, r#"{"action":"complete","message":"done"}"#);
    }

    #[test]
    fn extract_json_falls_back_to_markdown_fence_when_markers_absent() {
        let raw = "```json\n{\"action\":\"complete\",\"message\":\"hi\"}\n```";
        let result = extract_json(raw);
        assert_eq!(result, r#"{"action":"complete","message":"hi"}"#);
    }

    #[test]
    fn extract_json_returns_trimmed_raw_when_no_markers_or_fence() {
        let raw = r#"  {"action":"complete","message":"ok"}  "#;
        let result = extract_json(raw);
        assert_eq!(result, r#"{"action":"complete","message":"ok"}"#);
    }

    #[test]
    fn parse_controller_action_accepts_next_step_top_level_tool_payload() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "Check weather" },
            "tool": "weather",
            "args": { "location": "Austin, TX" }
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { tool, .. } => {
                assert_eq!(tool.as_deref(), Some("weather"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_next_step_tool_payload_with_string_args() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "Check weather" },
            "tool": "weather",
            "args": r#"{"location": "Austin, TX"}"#
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { tool, .. } => {
                assert_eq!(tool.as_deref(), Some("weather"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_next_step_step_hoisted_to_top_level() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "Check weather" },
            "step": {
                "tool": "weather",
                "args": { "location": "Austin, TX" }
            }
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { tool, .. } => {
                assert_eq!(tool.as_deref(), Some("weather"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_rejects_next_step_with_only_thinking() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "pondering" }
        });
        let result = parse_controller_action(&value);
        assert!(result.is_err(), "should reject next_step with only thinking");
    }

    #[test]
    fn parse_controller_action_rejects_blank_question_without_tool_or_message() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "hmm" },
            "question": "  "
        });
        let result = parse_controller_action(&value);
        assert!(result.is_err(), "should reject blank question");
    }

    #[test]
    fn parse_controller_action_next_step_with_message_infers_respond() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "respond" },
            "message": "Here is my answer"
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { message, .. } => {
                assert_eq!(message.as_deref(), Some("Here is my answer"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_does_not_synthesize_tool_from_thinking() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "I want to call weather tool" }
        });
        let result = parse_controller_action(&value);
        assert!(result.is_err(), "should not synthesize tool from thinking");
    }

    #[test]
    fn parse_controller_action_infers_tool_type_from_tool_field() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "fetch" },
            "tool": "web.fetch",
            "args": "{}"
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { tool, .. } => {
                assert_eq!(tool.as_deref(), Some("web.fetch"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_tool_name_alias() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "fetch" },
            "tool_name": "web.fetch",
            "args": "{}"
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { tool, .. } => {
                assert_eq!(tool.as_deref(), Some("web.fetch"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_args_aliases() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "fetch" },
            "tool": "web.fetch",
            "arguments": { "url": "https://example.com" }
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { args, .. } => {
                assert!(args.get("url").is_some(), "args should have url from arguments alias");
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_handles_respond_action_as_complete() {
        let value = json!({
            "action": "respond",
            "message": "Here is the result"
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::Complete { message } => {
                assert_eq!(message, "Here is the result");
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_handles_respond_action_with_response_alias() {
        let value = json!({
            "action": "respond",
            "response": "Here is the result"
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::Complete { message } => {
                assert_eq!(message, "Here is the result");
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_message_alias() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "respond" },
            "response": "Here is the answer"
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { message, .. } => {
                assert_eq!(message.as_deref(), Some("Here is the answer"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_accepts_valid_output_mode_with_string_args() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "fetch" },
            "tool": "web.fetch",
            "args": "{}",
            "output_mode": "persist"
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { output_mode, .. } => {
                assert_eq!(output_mode.as_deref(), Some("persist"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_rejects_invalid_output_mode() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "fetch" },
            "tool": "web.fetch",
            "args": "{}",
            "output_mode": "streaming"
        });
        let result = parse_controller_action(&value);
        assert!(result.is_err(), "should reject invalid output_mode");
    }

    #[test]
    fn parse_controller_action_accepts_tool_batch_with_per_tool_output_mode() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "batch" },
            "tools": [
                { "tool": "web.fetch", "args": "{}", "output_mode": "inline" },
                { "tool": "web.fetch", "args": "{}", "output_mode": "persist" }
            ]
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { tools, .. } => {
                let entries = tools.expect("tools");
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].output_mode.as_deref(), Some("inline"));
                assert_eq!(entries[1].output_mode.as_deref(), Some("persist"));
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_normalizes_tool_batch_aliases() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "batch" },
            "tool_calls": [
                { "tool_name": "web.fetch", "arguments": "{}" }
            ]
        });
        let action = parse_controller_action(&value).expect("parse");
        match action {
            ControllerAction::NextStep { tools, .. } => {
                let entries = tools.expect("tools");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].tool, "web.fetch");
            }
            other => panic!("expected NextStep, got {other:?}"),
        }
    }

    #[test]
    fn parse_controller_action_rejects_invalid_output_mode_in_tool_batch_item() {
        let value = json!({
            "action": "next_step",
            "thinking": { "task": "batch" },
            "tools": [
                { "tool": "web.fetch", "args": "{}", "output_mode": "streaming" }
            ]
        });
        let result = parse_controller_action(&value);
        assert!(result.is_err(), "should reject invalid output_mode in batch item");
    }

    #[test]
    fn controller_output_schema_has_no_one_of() {
        let schema = controller_output_format();
        let text = serde_json::to_string(&schema).unwrap();
        assert!(!text.contains("oneOf"), "schema must not contain oneOf");
        assert!(!text.contains("anyOf"), "schema must not contain anyOf");
        assert!(!text.contains("allOf"), "schema must not contain allOf");
    }

    #[test]
    fn controller_output_schema_has_flat_type_field() {
        let schema = controller_output_format();
        let props = schema
            .get("schema")
            .and_then(|s| s.get("properties"))
            .expect("properties");
        assert!(props.get("type").is_some(), "schema must have flat 'type' field");
    }

    #[test]
    fn controller_output_schema_includes_output_mode_enum() {
        let schema = controller_output_format();
        let output_mode = schema
            .get("schema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.get("output_mode"))
            .expect("output_mode property");
        let enum_values = output_mode.get("enum").expect("output_mode enum");
        let values: Vec<&str> = enum_values
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(values.contains(&"auto"));
        assert!(values.contains(&"inline"));
        assert!(values.contains(&"persist"));
    }

    #[test]
    fn controller_output_schema_includes_tool_batch_shape() {
        let schema = controller_output_format();
        let tools = schema
            .get("schema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.get("tools"))
            .expect("tools property");
        assert_eq!(
            tools.get("type").and_then(|t| t.as_str()),
            Some("array"),
            "tools must be array type"
        );
        let items = tools.get("items").expect("tools items");
        let item_props = items.get("properties").expect("tool item properties");
        assert!(item_props.get("tool").is_some(), "tool item must have 'tool' field");
        assert!(item_props.get("args").is_some(), "tool item must have 'args' field");
        assert!(item_props.get("output_mode").is_some(), "tool item must have 'output_mode' field");
    }

    fn check_no_conditional_keywords(value: &Value, path: &str) {
        let forbidden = ["oneOf", "anyOf", "allOf", "if", "then", "else", "not"];
        if let Value::Object(map) = value {
            for key in map.keys() {
                assert!(
                    !forbidden.contains(&key.as_str()),
                    "Found forbidden keyword '{key}' at {path}"
                );
            }
            for (key, child) in map {
                check_no_conditional_keywords(child, &format!("{path}.{key}"));
            }
        } else if let Value::Array(arr) = value {
            for (idx, child) in arr.iter().enumerate() {
                check_no_conditional_keywords(child, &format!("{path}[{idx}]"));
            }
        }
    }

    #[test]
    fn controller_schema_has_no_conditional_keywords_at_any_depth() {
        let schema = controller_output_format();
        check_no_conditional_keywords(&schema, "$");
    }

    #[test]
    fn controller_schema_root_has_additional_properties_false() {
        let schema = controller_output_format();
        let root = schema.get("schema").expect("schema");
        assert_eq!(
            root.get("additionalProperties"),
            Some(&json!(false)),
            "root must have additionalProperties: false"
        );
    }

    #[test]
    fn controller_schema_args_field_is_string_type() {
        let schema = controller_output_format();
        let args = schema
            .get("schema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.get("args"))
            .expect("args property");
        assert_eq!(
            args.get("type").and_then(|t| t.as_str()),
            Some("string"),
            "args field must be string type"
        );
    }

    #[test]
    fn controller_schema_confidence_has_no_numeric_bounds() {
        let schema = controller_output_format();
        let confidence = schema
            .get("schema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.get("thinking"))
            .and_then(|t| t.get("properties"))
            .and_then(|p| p.get("confidence"))
            .expect("confidence property");
        assert!(confidence.get("minimum").is_none(), "confidence must have no minimum");
        assert!(confidence.get("maximum").is_none(), "confidence must have no maximum");
    }

    #[test]
    fn controller_schema_survives_anthropic_sanitizer_with_known_diff() {
        let format = controller_output_format();
        let mut sanitized = format.clone();
        if let Some(schema) = sanitized.get_mut("schema") {
            crate::llm::strip_anthropic_unsupported_schema_keywords(schema);
        }
        crate::llm::validate_anthropic_output_format(Some(&sanitized)).expect("should pass anthropic validation");
    }

    #[test]
    fn controller_schema_passes_anthropic_validation() {
        let format = controller_output_format();
        let mut sanitized = format.clone();
        if let Some(schema) = sanitized.get_mut("schema") {
            crate::llm::strip_anthropic_unsupported_schema_keywords(schema);
        }
        crate::llm::validate_anthropic_output_format(Some(&sanitized)).expect("should pass preflight");
    }
}

pub fn controller_output_format() -> Value {
    json_schema_output_format(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["action"],
        "properties": {
            "action": {
                "type": "string",
                "enum": ["next_step", "complete", "guardrail_stop", "ask_user"]
            },
            "thinking": {
                "type": "object",
                "properties": {
                    "task": { "type": "string" },
                    "facts": { "type": "array", "items": { "type": "string" } },
                    "decisions": { "type": "array", "items": { "type": "string" } },
                    "risks": { "type": "array", "items": { "type": "string" } },
                    "confidence": { "type": "number" }
                },
                "additionalProperties": true
            },
            "type": {
                "type": "string",
                "enum": ["tool", "tool_batch", "respond", "ask_user"]
            },
            "description": { "type": "string" },
            "tool": { "type": "string" },
            "tools": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "tool": { "type": "string" },
                        "args": { "type": "string" },
                        "output_mode": {
                            "type": "string",
                            "enum": ["auto", "inline", "persist"]
                        }
                    },
                    "required": ["tool"],
                    "additionalProperties": false
                }
            },
            // NOTE: Anthropic structured outputs enforce `additionalProperties: false` on object schemas.
            // If we typed this as `object` without explicit properties/patternProperties, the LLM would be
            // forced to emit `{}` for args. Encode tool args as JSON text and parse via `normalize_tool_args`.
            "args": { "type": "string" },
            "output_mode": {
                "type": "string",
                "enum": ["auto", "inline", "persist"]
            },
            "message": { "type": "string" },
            "reason": { "type": "string" },
            "question": { "type": "string" },
            "context": { "type": "string" },
            "resume_to": {
                "type": "string",
                "enum": ["reflecting", "controller"]
            }
        },
        "additionalProperties": false
    }))
}
