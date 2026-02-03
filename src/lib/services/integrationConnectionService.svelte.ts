import { invoke } from "@tauri-apps/api/tauri";
import type {
    IntegrationConnection,
    CreateIntegrationConnectionInput,
    UpdateIntegrationConnectionInput
} from "$lib/types/integrationConnection";

export class IntegrationConnectionService {
    connections = $state<IntegrationConnection[]>([]);
    loading = $state<boolean>(false);
    error = $state<string | null>(null);

    public async loadConnections(): Promise<IntegrationConnection[]> {
        this.loading = true;
        this.error = null;
        try {
            const connections = await invoke<IntegrationConnection[]>("get_integration_connections");
            this.connections = connections;
            return connections;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return [];
        } finally {
            this.loading = false;
        }
    }

    public async createConnection(
        input: CreateIntegrationConnectionInput
    ): Promise<IntegrationConnection | null> {
        this.loading = true;
        this.error = null;
        try {
            const connection = await invoke<IntegrationConnection>("create_integration_connection", { input });
            this.connections = [connection, ...this.connections];
            return connection;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return null;
        } finally {
            this.loading = false;
        }
    }

    public async updateConnection(
        input: UpdateIntegrationConnectionInput
    ): Promise<IntegrationConnection | null> {
        this.loading = true;
        this.error = null;
        try {
            const connection = await invoke<IntegrationConnection | null>("update_integration_connection", { input });
            if (connection) {
                this.connections = this.connections.map((item) => item.id === connection.id ? connection : item);
            }
            return connection;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return null;
        } finally {
            this.loading = false;
        }
    }

    public async deleteConnection(id: string): Promise<boolean> {
        this.loading = true;
        this.error = null;
        try {
            const success = await invoke<boolean>("delete_integration_connection", { id });
            if (success) {
                this.connections = this.connections.filter((item) => item.id !== id);
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

    public async testConnection(id: string): Promise<{ ok: boolean; status: number } | null> {
        this.loading = true;
        this.error = null;
        try {
            const result = await invoke<{ ok: boolean; status: number }>("test_integration_connection", { id });
            return result;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            return null;
        } finally {
            this.loading = false;
        }
    }

    public getConnectionsForIntegration(integrationId: string): IntegrationConnection[] {
        return this.connections.filter((item) => item.integration_id === integrationId);
    }
}

export const integrationConnectionService = new IntegrationConnectionService();
