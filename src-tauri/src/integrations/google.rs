use super::IntegrationMetadata;

pub fn integrations() -> Vec<IntegrationMetadata> {
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
            description: "Read, create, and edit calendar events.".to_string(),
            auth_type: "oauth2".to_string(),
            category: "calendar".to_string(),
            capabilities: vec![
                "sync".to_string(),
                "action_execute".to_string(),
                "discovery".to_string(),
            ],
        },
    ]
}
