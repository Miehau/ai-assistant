export type McpTransport = "stdio" | "streamable_http";
export type McpAuthMode = "auto" | "none" | "bearer" | "oauth";
export type McpAuthStatus = "not_required" | "required" | "pending" | "authorized" | "error";
export type McpConnectionStatus = "disabled" | "connecting" | "connected" | "error";
export type McpOAuthSessionStatus = "pending" | "authorized" | "denied" | "expired" | "cancelled" | "error";

export interface McpOAuthSession {
    id: string;
    serverId: string;
    status: McpOAuthSessionStatus;
    authorizationUrl?: string;
    expiresAt: number;
    error: string | null;
}

export interface McpServer {
    id: string;
    name: string;
    transport: McpTransport;
    command: string;
    args: string[];
    env: Record<string, string>;
    cwd: string | null;
    url: string;
    bearerTokenConfigured: boolean;
    authMode: McpAuthMode;
    authStatus: McpAuthStatus;
    connectionStatus: McpConnectionStatus;
    oauthCredentialsConfigured: boolean;
    oauthSession: McpOAuthSession | null;
    enabled: boolean;
    status: "disabled" | "connecting" | "connected" | "error";
    error: string | null;
    createdAt: number;
    updatedAt: number;
    tools: McpTool[];
}

export interface McpTool {
    id: string;
    serverId: string;
    remoteName: string;
    registeredName: string;
    description: string;
    inputSchema: Record<string, unknown>;
    enabledForNewSessions: boolean;
    createdAt: number;
    updatedAt: number;
}

export interface CreateMcpServerInput {
    name: string;
    transport?: McpTransport;
    command?: string | null;
    args?: string[];
    env?: Record<string, string>;
    cwd?: string | null;
    url?: string | null;
    bearerToken?: string | null;
    authMode?: McpAuthMode;
    enabled?: boolean;
}

export interface UpdateMcpServerInput extends Partial<CreateMcpServerInput> {
    id: string;
}

export interface McpOAuthStartResponse {
    session: McpOAuthSession & { authorizationUrl: string };
}

export interface McpOAuthSessionResponse {
    session: McpOAuthSession | null;
}
