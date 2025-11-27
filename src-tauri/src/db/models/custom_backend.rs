use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomBackend {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCustomBackendInput {
    pub name: String,
    pub url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateCustomBackendInput {
    pub id: String,
    pub name: Option<String>,
    pub url: Option<String>,
    pub api_key: Option<String>,
}
