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
        console.warn('[integrationConnectionService] loadConnections not yet implemented in server backend');
        return [];
    }

    public async createConnection(
        input: CreateIntegrationConnectionInput
    ): Promise<IntegrationConnection | null> {
        throw new Error('Not yet implemented in server backend');
    }

    public async updateConnection(
        input: UpdateIntegrationConnectionInput
    ): Promise<IntegrationConnection | null> {
        throw new Error('Not yet implemented in server backend');
    }

    public async deleteConnection(id: string): Promise<boolean> {
        throw new Error('Not yet implemented in server backend');
    }

    public async testConnection(id: string): Promise<{ ok: boolean; status: number } | null> {
        console.warn('[integrationConnectionService] testConnection not yet implemented in server backend');
        return null;
    }

    public getConnectionsForIntegration(integrationId: string): IntegrationConnection[] {
        return this.connections.filter((item) => item.integration_id === integrationId);
    }
}

export const integrationConnectionService = new IntegrationConnectionService();
