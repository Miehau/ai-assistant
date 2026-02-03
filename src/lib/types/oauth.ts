export interface OAuthStartResponse {
    session_id: string;
    auth_url: string;
}

export interface OAuthSessionStatus {
    status: string;
    connection_id?: string;
    error?: string;
}
