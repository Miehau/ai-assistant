use super::IntegrationMetadata;

pub fn integrations() -> Vec<IntegrationMetadata> {
    vec![IntegrationMetadata {
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
    }]
}
