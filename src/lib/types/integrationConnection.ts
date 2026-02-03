export interface IntegrationConnection {
    id: string;
    integration_id: string;
    account_label?: string;
    status: string;
    auth_type: string;
    access_token?: string;
    refresh_token?: string;
    scopes?: string;
    expires_at?: number;
    last_error?: string;
    last_sync_at?: number;
    created_at: number;
    updated_at: number;
}

export interface CreateIntegrationConnectionInput {
    integration_id: string;
    account_label?: string;
    auth_type: string;
    access_token?: string;
    refresh_token?: string;
    scopes?: string;
    expires_at?: number;
}

export interface UpdateIntegrationConnectionInput {
    id: string;
    account_label?: string;
    status?: string;
    auth_type?: string;
    access_token?: string;
    refresh_token?: string;
    scopes?: string;
    expires_at?: number;
    last_error?: string;
    last_sync_at?: number;
}
