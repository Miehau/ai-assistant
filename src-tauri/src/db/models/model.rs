use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Model {
    pub provider: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_name: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    /// For custom backends, the ID of the custom backend configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_backend_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct ApiKey {
    pub provider: String,
    pub key: String,
} 