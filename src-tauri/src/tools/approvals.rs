use crate::db::{Db, PreferenceOperations};
use std::collections::HashMap;

pub const PREF_TOOL_APPROVAL_OVERRIDES: &str = "plugins.tools.approval_overrides";
pub const PREF_TOOL_CONVERSATION_APPROVAL_OVERRIDES: &str =
    "plugins.tools.conversation_approval_overrides";

pub fn load_tool_approval_overrides(db: &Db) -> Result<HashMap<String, bool>, String> {
    let raw = PreferenceOperations::get_preference(db, PREF_TOOL_APPROVAL_OVERRIDES)
        .map_err(|err| format!("Failed to read tool approval overrides: {err}"))?;
    let Some(raw) = raw else {
        return Ok(HashMap::new());
    };

    match serde_json::from_str::<HashMap<String, bool>>(&raw) {
        Ok(map) => Ok(map),
        Err(err) => {
            log::warn!("Failed to parse tool approval overrides, using defaults: {err}");
            Ok(HashMap::new())
        }
    }
}

pub fn get_tool_approval_override(db: &Db, tool_name: &str) -> Result<Option<bool>, String> {
    let overrides = load_tool_approval_overrides(db)?;
    Ok(overrides.get(tool_name).copied())
}

pub fn set_tool_approval_override(
    db: &Db,
    tool_name: &str,
    requires_approval: Option<bool>,
) -> Result<(), String> {
    let mut overrides = load_tool_approval_overrides(db)?;
    match requires_approval {
        Some(value) => {
            overrides.insert(tool_name.to_string(), value);
        }
        None => {
            overrides.remove(tool_name);
        }
    }

    let serialized = serde_json::to_string(&overrides)
        .map_err(|err| format!("Failed to serialize tool approval overrides: {err}"))?;
    PreferenceOperations::set_preference(db, PREF_TOOL_APPROVAL_OVERRIDES, &serialized)
        .map_err(|err| format!("Failed to save tool approval overrides: {err}"))?;
    Ok(())
}

type ConversationToolApprovalOverrides = HashMap<String, HashMap<String, bool>>;

fn load_all_conversation_tool_approval_overrides(
    db: &Db,
) -> Result<ConversationToolApprovalOverrides, String> {
    let raw =
        PreferenceOperations::get_preference(db, PREF_TOOL_CONVERSATION_APPROVAL_OVERRIDES)
            .map_err(|err| format!("Failed to read conversation tool approval overrides: {err}"))?;
    let Some(raw) = raw else {
        return Ok(HashMap::new());
    };

    match serde_json::from_str::<ConversationToolApprovalOverrides>(&raw) {
        Ok(map) => Ok(map),
        Err(err) => {
            log::warn!(
                "Failed to parse conversation tool approval overrides, using defaults: {err}"
            );
            Ok(HashMap::new())
        }
    }
}

pub fn load_conversation_tool_approval_overrides(
    db: &Db,
    conversation_id: &str,
) -> Result<HashMap<String, bool>, String> {
    let all_overrides = load_all_conversation_tool_approval_overrides(db)?;
    Ok(all_overrides
        .get(conversation_id)
        .cloned()
        .unwrap_or_default())
}

pub fn get_conversation_tool_approval_override(
    db: &Db,
    conversation_id: &str,
    tool_name: &str,
) -> Result<Option<bool>, String> {
    let overrides = load_conversation_tool_approval_overrides(db, conversation_id)?;
    Ok(overrides.get(tool_name).copied())
}

pub fn set_conversation_tool_approval_override(
    db: &Db,
    conversation_id: &str,
    tool_name: &str,
    requires_approval: Option<bool>,
) -> Result<(), String> {
    let mut all_overrides = load_all_conversation_tool_approval_overrides(db)?;
    let conversation_entry = all_overrides
        .entry(conversation_id.to_string())
        .or_default();

    match requires_approval {
        Some(value) => {
            conversation_entry.insert(tool_name.to_string(), value);
        }
        None => {
            conversation_entry.remove(tool_name);
        }
    }

    if conversation_entry.is_empty() {
        all_overrides.remove(conversation_id);
    }

    let serialized = serde_json::to_string(&all_overrides).map_err(|err| {
        format!("Failed to serialize conversation tool approval overrides: {err}")
    })?;
    PreferenceOperations::set_preference(
        db,
        PREF_TOOL_CONVERSATION_APPROVAL_OVERRIDES,
        &serialized,
    )
    .map_err(|err| format!("Failed to save conversation tool approval overrides: {err}"))?;
    Ok(())
}
