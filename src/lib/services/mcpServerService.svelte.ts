import type { McpServer, CreateMcpServerInput, UpdateMcpServerInput } from "$lib/types/mcpServer";
import { backend } from "$lib/backend/client";

export class McpServerService {
    servers = $state<McpServer[]>([]);
    loading = $state<boolean>(false);
    error = $state<string | null>(null);

    public async loadServers(): Promise<McpServer[]> {
        this.loading = true;
        this.error = null;
        try {
            this.servers = (await backend.getMcpServers()).map(normalizeServer);
            return this.servers;
        } catch (error) {
            this.error = error instanceof Error ? error.message : String(error);
            return [];
        } finally {
            this.loading = false;
        }
    }

    public async getServer(id: string): Promise<McpServer | null> {
        const server = this.servers.find((item) => item.id === id) ?? await backend.getMcpServer(id);
        return server ? normalizeServer(server) : null;
    }

    public async createServer(input: CreateMcpServerInput): Promise<McpServer | null> {
        const server = await backend.createMcpServer(normalizeCreateInput(input));
        await this.loadServers();
        return normalizeServer(server);
    }

    public async updateServer(input: UpdateMcpServerInput): Promise<McpServer | null> {
        const server = await backend.updateMcpServer(normalizeUpdateInput(input));
        await this.loadServers();
        return server ? normalizeServer(server) : null;
    }

    public async deleteServer(id: string): Promise<boolean> {
        const deleted = await backend.deleteMcpServer(id);
        await this.loadServers();
        return deleted;
    }

    public async testServer(id: string): Promise<{ ok: boolean; status: number } | null> {
        const result = await backend.testMcpServer(id);
        await this.loadServers();
        return result;
    }

    public async setToolEnabled(serverId: string, toolName: string, enabledForNewSessions: boolean): Promise<void> {
        await backend.setMcpToolEnabled(serverId, toolName, enabledForNewSessions);
        await this.loadServers();
    }

    public getServerByName(name: string): McpServer | undefined {
        return this.servers.find((item) => item.name === name);
    }

    public getAllServers(): McpServer[] {
        return [...this.servers];
    }
}

export const mcpServerService = new McpServerService();

function normalizeServer(server: McpServer): McpServer {
    return {
        ...server,
        command: server.command ?? "",
        url: server.url ?? "",
        auth_type: server.auth_type ?? (server.bearerTokenConfigured ? "api_key" : "none"),
        created_at: server.created_at ?? server.createdAt,
    };
}

function normalizeCreateInput(input: CreateMcpServerInput): CreateMcpServerInput {
    return {
        ...input,
        transport: input.transport ?? "streamable_http",
        bearerToken: input.bearerToken ?? input.api_key ?? null,
        enabled: input.enabled ?? true,
    };
}

function normalizeUpdateInput(input: UpdateMcpServerInput): UpdateMcpServerInput {
    return {
        ...input,
        bearerToken: input.bearerToken ?? input.api_key,
    };
}
