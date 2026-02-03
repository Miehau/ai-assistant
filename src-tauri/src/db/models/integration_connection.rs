use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct IntegrationConnection {
    pub id: String,
    pub integration_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_label: Option<String>,
    pub status: String,
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct CreateIntegrationConnectionInput {
    pub integration_id: String,
    pub account_label: Option<String>,
    pub auth_type: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub scopes: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct UpdateIntegrationConnectionInput {
    pub id: String,
    pub account_label: Option<String>,
    pub status: Option<String>,
    pub auth_type: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub scopes: Option<String>,
    pub expires_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_sync_at: Option<i64>,
}
