use crate::db::{Db, PreferenceOperations};
use std::collections::HashMap;

pub const PREF_TOOL_APPROVAL_OVERRIDES: &str = "plugins.tools.approval_overrides";

pub fn load_tool_approval_overrides(db: &Db) -> Result<HashMap<String, bool>, String> {
    let raw = PreferenceOperations::get_preference(db, PREF_TOOL_APPROVAL_OVERRIDES)
        .map_err(|err| format!("Failed to read tool approval overrides: {err}"))?;
    let Some(raw) = raw else {
        return Ok(HashMap::new());
    };

    match serde_json::from_str::<HashMap<String, bool>>(&raw) {
        Ok(map) => Ok(map),
        Err(err) => {
            log::warn!(
                "Failed to parse tool approval overrides, using defaults: {err}"
            );
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
