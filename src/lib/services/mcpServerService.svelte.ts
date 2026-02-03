import { invoke } from "@tauri-apps/api/tauri";
import type { McpServer, CreateMcpServerInput, UpdateMcpServerInput } from "$lib/types/mcpServer";

export class McpServerService {
    servers = $state<McpServer[]>([]);
    loading = $state<boolean>(false);
    error = $state<string | null>(null);

    public async loadServers(): Promise<McpServer[]> {
        this.loading = true;
        this.error = null;

        try {
            const servers = await invoke<McpServer[]>("get_mcp_servers");
            this.servers = servers;
            return servers;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return [];
        } finally {
            this.loading = false;
        }
    }

    public async getServer(id: string): Promise<McpServer | null> {
        try {
            const server = await invoke<McpServer | null>("get_mcp_server", { id });
            return server;
        } catch (error) {
            return null;
        }
    }

    public async createServer(input: CreateMcpServerInput): Promise<McpServer | null> {
        this.loading = true;
        this.error = null;

        try {
            const server = await invoke<McpServer>("create_mcp_server", { input });
            this.servers = [...this.servers, server];
            return server;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return null;
        } finally {
            this.loading = false;
        }
    }

    public async updateServer(input: UpdateMcpServerInput): Promise<McpServer | null> {
        this.loading = true;
        this.error = null;

        try {
            const server = await invoke<McpServer | null>("update_mcp_server", { input });
            if (server) {
                this.servers = this.servers.map((item) => item.id === server.id ? server : item);
            }
            return server;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return null;
        } finally {
            this.loading = false;
        }
    }

    public async deleteServer(id: string): Promise<boolean> {
        this.loading = true;
        this.error = null;

        try {
            const success = await invoke<boolean>("delete_mcp_server", { id });
            if (success) {
                this.servers = this.servers.filter((item) => item.id !== id);
            }
            return success;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return false;
        } finally {
            this.loading = false;
        }
    }

    public async testServer(id: string): Promise<{ ok: boolean; status: number } | null> {
        this.loading = true;
        this.error = null;

        try {
            const result = await invoke<{ ok: boolean; status: number }>("test_mcp_server", { id });
            return result;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return null;
        } finally {
            this.loading = false;
        }
    }

    public getServerByName(name: string): McpServer | undefined {
        return this.servers.find((item) => item.name === name);
    }

    public getAllServers(): McpServer[] {
        return [...this.servers];
    }
}

export const mcpServerService = new McpServerService();
