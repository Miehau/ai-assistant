use serde::{Deserialize, Serialize};
use specta::Type;

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
    vec![
        IntegrationMetadata {
            id: "gmail".to_string(),
            name: "Gmail".to_string(),
            provider: "google".to_string(),
            description: "Read and send email, manage labels.".to_string(),
            auth_type: "oauth2".to_string(),
            category: "email".to_string(),
            capabilities: vec![
                "sync".to_string(),
                "webhook_ingest".to_string(),
                "action_execute".to_string(),
                "discovery".to_string(),
            ],
        },
        IntegrationMetadata {
            id: "google_calendar".to_string(),
            name: "Google Calendar".to_string(),
            provider: "google".to_string(),
            description: "Read and create calendar events.".to_string(),
            auth_type: "oauth2".to_string(),
            category: "calendar".to_string(),
            capabilities: vec![
                "sync".to_string(),
                "action_execute".to_string(),
                "discovery".to_string(),
            ],
        },
        IntegrationMetadata {
            id: "todoist".to_string(),
            name: "Todoist".to_string(),
            provider: "todoist".to_string(),
            description: "Create and complete personal tasks.".to_string(),
            auth_type: "oauth2".to_string(),
            category: "tasks".to_string(),
            capabilities: vec![
                "sync".to_string(),
                "action_execute".to_string(),
                "discovery".to_string(),
            ],
        },
        IntegrationMetadata {
            id: "mcp".to_string(),
            name: "MCP Servers".to_string(),
            provider: "mcp".to_string(),
            description: "Configure local or remote MCP servers.".to_string(),
            auth_type: "api_key".to_string(),
            category: "mcp".to_string(),
            capabilities: vec![
                "discovery".to_string(),
                "health_check".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::default_integrations;

    #[test]
    fn default_integrations_include_core_providers() {
        let integrations = default_integrations();
        let ids = integrations.iter().map(|item| item.id.as_str()).collect::<Vec<_>>();
        assert!(ids.contains(&"gmail"));
        assert!(ids.contains(&"google_calendar"));
        assert!(ids.contains(&"todoist"));
    }
}
