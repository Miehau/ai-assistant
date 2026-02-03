use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct CreateMcpServerInput {
    pub name: String,
    pub url: String,
    pub auth_type: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct UpdateMcpServerInput {
    pub id: String,
    pub name: Option<String>,
    pub url: Option<String>,
    pub auth_type: Option<String>,
    pub api_key: Option<String>,
}
