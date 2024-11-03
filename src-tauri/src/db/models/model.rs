use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Model {
    pub provider: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_name: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKey {
    pub provider: String,
    pub key: String,
} 