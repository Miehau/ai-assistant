use serde::{Deserialize, Serialize};
use specta::Type;

pub mod google;
pub mod mcp;
pub mod todoist;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct IntegrationMetadata {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub description: String,
    pub auth_type: String,
    pub category: String,
    pub capabilities: Vec<String>,
}

pub fn default_integrations() -> Vec<IntegrationMetadata> {
    let mut integrations = Vec::new();
    integrations.extend(google::integrations());
    integrations.extend(todoist::integrations());
    integrations.extend(mcp::integrations());
    integrations
}

#[cfg(test)]
mod tests {
    use super::default_integrations;

    #[test]
    fn default_integrations_include_core_providers() {
        let integrations = default_integrations();
        let ids = integrations
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"gmail"));
        assert!(ids.contains(&"google_calendar"));
        assert!(ids.contains(&"todoist"));
        assert!(ids.contains(&"mcp"));
    }
}
