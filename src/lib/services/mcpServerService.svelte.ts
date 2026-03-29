import type { McpServer, CreateMcpServerInput, UpdateMcpServerInput } from "$lib/types/mcpServer";

export class McpServerService {
    servers = $state<McpServer[]>([]);
    loading = $state<boolean>(false);
    error = $state<string | null>(null);

    public async loadServers(): Promise<McpServer[]> {
        console.warn('[mcpServerService] loadServers not yet implemented in server backend');
        return [];
    }

    public async getServer(id: string): Promise<McpServer | null> {
        console.warn('[mcpServerService] getServer not yet implemented in server backend');
        return null;
    }

    public async createServer(input: CreateMcpServerInput): Promise<McpServer | null> {
        throw new Error('Not yet implemented in server backend');
    }

    public async updateServer(input: UpdateMcpServerInput): Promise<McpServer | null> {
        throw new Error('Not yet implemented in server backend');
    }

    public async deleteServer(id: string): Promise<boolean> {
        throw new Error('Not yet implemented in server backend');
    }

    public async testServer(id: string): Promise<{ ok: boolean; status: number } | null> {
        console.warn('[mcpServerService] testServer not yet implemented in server backend');
        return null;
    }

    public getServerByName(name: string): McpServer | undefined {
        return this.servers.find((item) => item.name === name);
    }

    public getAllServers(): McpServer[] {
        return [...this.servers];
    }
}

export const mcpServerService = new McpServerService();
