export interface McpServer {
    id: string;
    name: string;
    transport: "stdio" | "streamable_http";
    command: string;
    args: string[];
    env: Record<string, string>;
    cwd: string | null;
    url: string;
    bearerTokenConfigured: boolean;
    enabled: boolean;
    status: "disabled" | "connected" | "error";
    error: string | null;
    createdAt: number;
    updatedAt: number;
    tools: McpTool[];
    auth_type: string;
    api_key?: string;
    created_at: number;
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
    transport?: "stdio" | "streamable_http";
    command?: string | null;
    args?: string[];
    env?: Record<string, string>;
    cwd?: string | null;
    url?: string | null;
    bearerToken?: string | null;
    enabled?: boolean;
    auth_type?: string;
    api_key?: string;
}

export interface UpdateMcpServerInput {
    id: string;
    name?: string;
    transport?: "stdio" | "streamable_http";
    command?: string | null;
    args?: string[];
    env?: Record<string, string>;
    cwd?: string | null;
    url?: string | null;
    bearerToken?: string | null;
    enabled?: boolean;
    auth_type?: string;
    api_key?: string;
}
