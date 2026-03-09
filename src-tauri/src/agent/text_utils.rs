use serde_json::Value;

use crate::llm::LlmMessage;

pub fn truncate_chars(input: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), !input.is_empty());
    }

    let mut output = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            return (output, true);
        }
        output.push(ch);
    }
    (output, false)
}

pub fn truncate_with_notice(input: &str, max_chars: usize) -> String {
    let (truncated, was_truncated) = truncate_chars(input, max_chars);
    if was_truncated {
        format!("{truncated} ...(truncated)")
    } else {
        truncated
    }
}

pub fn summarize_tool_args(args: &Value, max_len: usize) -> String {
    let raw = serde_json::to_string(args).unwrap_or_else(|_| "<invalid-json>".to_string());
    if raw.len() <= max_len {
        return raw;
    }
    let truncated: String = raw.chars().take(max_len).collect();
    format!("{truncated}...")
}

pub fn summarize_tool_output_value(value: &Value, max_chars: usize) -> (String, bool) {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    truncate_chars(&serialized, max_chars)
}

pub fn value_char_len(value: &Value) -> usize {
    serde_json::to_string(value)
        .map(|text| text.chars().count())
        .unwrap_or(usize::MAX)
}

pub fn value_to_string(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(array) = value.as_array() {
        let mut combined = String::new();
        for entry in array {
            if let Some(text) = entry.get("text").and_then(|v| v.as_str()) {
                combined.push_str(text);
            }
        }
        return combined;
    }

    value.to_string()
}

pub fn summarize_goal(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return "Agent task".to_string();
    }
    let mut result = String::new();
    for ch in trimmed.chars().take(160) {
        result.push(ch);
    }
    result
}

pub fn compact_history_messages_with_limits(
    messages: &[LlmMessage],
    max_chars: usize,
    stable_prefix_messages: usize,
    recent_tail_messages: usize,
) -> Vec<LlmMessage> {
    let message_sizes: Vec<usize> = messages
        .iter()
        .map(|msg| value_to_string(&msg.content).chars().count())
        .collect();
    let total_chars: usize = message_sizes.iter().sum();

    if total_chars <= max_chars {
        return messages.to_vec();
    }

    let prefix_end = messages.len().min(stable_prefix_messages);
    let tail_start = messages.len().saturating_sub(recent_tail_messages);
    if tail_start <= prefix_end {
        return messages.to_vec();
    }

    let mut compacted = Vec::with_capacity(prefix_end + (messages.len() - tail_start));
    compacted.extend_from_slice(&messages[..prefix_end]);
    compacted.extend_from_slice(&messages[tail_start..]);
    compacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compacted_history_keeps_prefix_and_tail_without_summary_message() {
        let messages: Vec<LlmMessage> = (0..12)
            .map(|idx| LlmMessage {
                role: "user".to_string(),
                content: json!(format!("message-{idx}-{}", "x".repeat(48))),
            })
            .collect();

        let compacted = compact_history_messages_with_limits(&messages, 200, 2, 3);

        assert_eq!(compacted.len(), 5);
        assert_eq!(compacted[0].content, messages[0].content);
        assert_eq!(compacted[1].content, messages[1].content);
        assert_eq!(compacted[2].content, messages[9].content);
        assert_eq!(compacted[3].content, messages[10].content);
        assert_eq!(compacted[4].content, messages[11].content);

        for msg in &compacted {
            let text = value_to_string(&msg.content);
            assert!(
                !text.contains("[Context Summary:"),
                "compacted messages should not contain summary marker"
            );
        }
    }
}
