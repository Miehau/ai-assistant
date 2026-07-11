import type {
    McpOAuthSession,
    McpServer,
    CreateMcpServerInput,
    UpdateMcpServerInput,
} from "$lib/types/mcpServer";
import { backend } from "$lib/backend/client";
import {
    pendingMcpServerIds,
    pollMcpOAuthSession,
    normalizeMcpHttpError,
    type McpPollingOptions,
    type McpPollingOutcome,
} from "./mcpOAuthPolling";

type ServerListener = (servers: McpServer[]) => void;
export interface McpPollingHandle { promise: Promise<McpPollingOutcome>; cancel: () => void }

export class McpServerService {
    servers = $state<McpServer[]>([]);
    loading = $state(false);
    error = $state<string | null>(null);
    private listeners = new Set<ServerListener>();
    private pollers = new Map<string, AbortController>();

    public onServersChanged(listener: ServerListener): () => void {
        this.listeners.add(listener);
        listener([...this.servers]);
        return () => this.listeners.delete(listener);
    }

    public async loadServers(): Promise<McpServer[]> {
        this.loading = true;
        this.error = null;
        try {
            this.servers = (await backend.getMcpServers()).map(normalizeServer);
            this.notify();
            return this.servers;
        } catch (error) {
            this.error = normalizeMcpHttpError(error);
            return [];
        } finally {
            this.loading = false;
        }
    }

    public async getServer(id: string): Promise<McpServer | null> {
        const server = this.servers.find((item) => item.id === id) ?? await backend.getMcpServer(id);
        return server ? normalizeServer(server) : null;
    }

    public async createServer(input: CreateMcpServerInput): Promise<McpServer> {
        return this.change(() => backend.createMcpServer(normalizeCreateInput(input)));
    }

    public async updateServer(input: UpdateMcpServerInput): Promise<McpServer | null> {
        return this.change(() => backend.updateMcpServer(input));
    }

    public async deleteServer(id: string): Promise<boolean> {
        this.stopOAuthPolling(id);
        const deleted = await this.run(() => backend.deleteMcpServer(id));
        await this.loadServers();
        return deleted;
    }

    public async connect(id: string): Promise<McpServer> {
        return this.change(() => backend.connectMcpServer(id));
    }

    public async reconnect(id: string): Promise<McpServer> {
        return this.change(() => backend.reconnectMcpServer(id));
    }

    public async disconnect(id: string): Promise<McpServer> {
        this.stopOAuthPolling(id);
        return this.change(() => backend.disconnectMcpServer(id));
    }

    public async startOAuth(id: string): Promise<McpOAuthSession & { authorizationUrl: string }> {
        this.stopOAuthPolling(id);
        const { session } = await this.run(() => backend.startMcpOAuth(id));
        await this.loadServers();
        return session;
    }

    public async getOAuthSession(id: string, signal?: AbortSignal): Promise<McpOAuthSession | null> {
        return (await this.run(() => backend.getMcpOAuthSession(id, signal))).session;
    }

    public async cancelOAuth(id: string): Promise<McpServer> {
        this.stopOAuthPolling(id);
        return this.change(() => backend.cancelMcpOAuth(id));
    }

    /** Call only from the user's click/keyboard handler. A false result enables an explicit-link fallback. */
    public openAuthorization(
        session: McpOAuthSession & { authorizationUrl: string },
        opener: (url: string) => Window | null = (url) => window.open(url, "_blank", "noopener,noreferrer"),
    ): boolean {
        return Boolean(opener(session.authorizationUrl));
    }

    public pollOAuth(id: string, options: McpPollingOptions = {}): McpPollingHandle {
        this.stopOAuthPolling(id);
        const controller = new AbortController();
        this.pollers.set(id, controller);
        options.signal?.addEventListener("abort", () => controller.abort(), { once: true });
        let deadlineAborted = false;
        const timeoutTimer = setTimeout(() => {
            deadlineAborted = true;
            controller.abort();
        }, options.timeoutMs ?? 120_000);
        const promise = pollMcpOAuthSession(
            (signal) => this.getOAuthSession(id, signal),
            { ...options, signal: controller.signal },
        ).then(async (outcome) => {
            await this.loadServers();
            return deadlineAborted && outcome.reason === "cancelled"
                ? { ...outcome, reason: "timeout" as const }
                : outcome;
        }).catch((error) => {
            this.error = normalizeMcpHttpError(error);
            throw new Error(this.error);
        }).finally(() => {
            clearTimeout(timeoutTimer);
            if (this.pollers.get(id) === controller) this.pollers.delete(id);
        });
        return { promise, cancel: () => controller.abort() };
    }

    /** Reload-safe entrypoint: call when the management screen mounts. */
    public async restorePendingOAuth(options: McpPollingOptions = {}): Promise<string[]> {
        const ids = pendingMcpServerIds(await this.loadServers());
        for (const id of ids) void this.pollOAuth(id, options).promise.catch(() => undefined);
        return ids;
    }

    public stopOAuthPolling(id: string): void {
        this.pollers.get(id)?.abort();
        this.pollers.delete(id);
    }

    public dispose(): void {
        for (const controller of this.pollers.values()) controller.abort();
        this.pollers.clear();
    }

    public async testServer(id: string): Promise<{ ok: boolean; status: number }> {
        const result = await this.run(() => backend.testMcpServer(id));
        await this.loadServers();
        return result;
    }

    public async setToolEnabled(serverId: string, toolName: string, enabledForNewSessions: boolean): Promise<void> {
        await this.run(() => backend.setMcpToolEnabled(serverId, toolName, enabledForNewSessions));
        await this.loadServers();
    }

    public getServerByName(name: string): McpServer | undefined {
        return this.servers.find((item) => item.name === name);
    }

    public getAllServers(): McpServer[] {
        return [...this.servers];
    }

    private async change<T extends McpServer | null>(operation: () => Promise<T>): Promise<T> {
        const server = await this.run(operation);
        await this.loadServers();
        return server ? normalizeServer(server) as T : server;
    }

    private async run<T>(operation: () => Promise<T>): Promise<T> {
        this.error = null;
        try {
            return await operation();
        } catch (error) {
            this.error = normalizeMcpHttpError(error);
            throw new Error(this.error);
        }
    }

    private notify(): void {
        for (const listener of this.listeners) listener([...this.servers]);
    }
}

export const mcpServerService = new McpServerService();

function normalizeServer(server: McpServer): McpServer {
    return { ...server, command: server.command ?? "", url: server.url ?? "" };
}

function normalizeCreateInput(input: CreateMcpServerInput): CreateMcpServerInput {
    return {
        ...input,
        transport: input.transport ?? "streamable_http",
        authMode: input.authMode ?? "auto",
        enabled: input.enabled ?? false,
    };
}
