use super::IntegrationMetadata;

pub fn integrations() -> Vec<IntegrationMetadata> {
    vec![IntegrationMetadata {
        id: "mcp".to_string(),
        name: "MCP Servers".to_string(),
        provider: "mcp".to_string(),
        description: "Configure local or remote MCP servers.".to_string(),
        auth_type: "api_key".to_string(),
        category: "mcp".to_string(),
        capabilities: vec!["discovery".to_string(), "health_check".to_string()],
    }]
}
