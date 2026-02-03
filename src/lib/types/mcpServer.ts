export interface McpServer {
    id: string;
    name: string;
    url: string;
    auth_type: string;
    api_key?: string;
    created_at: number;
}

export interface CreateMcpServerInput {
    name: string;
    url: string;
    auth_type: string;
    api_key?: string;
}

export interface UpdateMcpServerInput {
    id: string;
    name?: string;
    url?: string;
    auth_type?: string;
    api_key?: string;
}
