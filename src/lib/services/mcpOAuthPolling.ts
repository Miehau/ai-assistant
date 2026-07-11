import type { McpOAuthSession, McpServer } from "../types/mcpServer";
import { HttpBackendError } from "../backend/http-client";

export type McpPollingOutcome =
    | { reason: "terminal"; session: McpOAuthSession | null }
    | { reason: "timeout"; session: McpOAuthSession | null }
    | { reason: "cancelled"; session: McpOAuthSession | null };

export interface McpPollingOptions {
    intervalMs?: number;
    timeoutMs?: number;
    signal?: AbortSignal;
    now?: () => number;
    sleep?: (ms: number, signal?: AbortSignal) => Promise<void>;
    onUpdate?: (session: McpOAuthSession | null) => void;
}

export async function pollMcpOAuthSession(
    fetchSession: (signal?: AbortSignal) => Promise<McpOAuthSession | null>,
    options: McpPollingOptions = {},
): Promise<McpPollingOutcome> {
    const now = options.now ?? Date.now;
    const sleep = options.sleep ?? abortableSleep;
    const deadline = now() + (options.timeoutMs ?? 120_000);
    let session: McpOAuthSession | null = null;

    while (true) {
        if (options.signal?.aborted) return { reason: "cancelled", session };
        session = await fetchSession(options.signal);
        options.onUpdate?.(session);
        if (!session || isTerminalMcpOAuthSession(session.status)) return { reason: "terminal", session };
        if (now() >= deadline) return { reason: "timeout", session };
        try {
            await sleep(Math.min(options.intervalMs ?? 1_000, Math.max(0, deadline - now())), options.signal);
        } catch (error) {
            if (options.signal?.aborted || (error instanceof DOMException && error.name === "AbortError")) {
                return { reason: "cancelled", session };
            }
            throw error;
        }
    }
}

export function isTerminalMcpOAuthSession(status: McpOAuthSession["status"]): boolean {
    return status !== "pending";
}

export function pendingMcpServerIds(servers: McpServer[], now = Date.now()): string[] {
    return servers
        .filter((server) => server.oauthSession?.status === "pending" && server.oauthSession.expiresAt > now)
        .map((server) => server.id);
}

export function mcpStatusLabel(server: Pick<McpServer, "authStatus" | "connectionStatus">): string {
    if (server.connectionStatus === "disabled") return "Disabled";
    if (server.authStatus === "pending") return "Waiting for authorization";
    if (server.authStatus === "required") return "Sign-in required";
    if (server.connectionStatus === "connected") return "Connected";
    return "Connection error";
}

export function normalizeMcpHttpError(error: unknown): string {
    if (error instanceof HttpBackendError) {
        if (error.status === 401) return "Backend authentication expired. Reconnect the backend and try again.";
        if (error.status === 404) return "This MCP server is unavailable or belongs to another user.";
        if (error.status === 409) return "This MCP action is no longer available. Refresh and try again.";
        if (error.message.includes("PUBLIC_BASE_URL")) return "MCP OAuth callback configuration is missing or unsafe.";
        if (error.message.includes("ENCRYPTION_KEY")) return "Secure MCP credential storage is not configured.";
        if (error.status === 429) return "Too many requests. Wait a moment and try again.";
    }
    return "Could not update the MCP connection. Check the server and try again.";
}

function abortableSleep(ms: number, signal?: AbortSignal): Promise<void> {
    return new Promise((resolve, reject) => {
        if (signal?.aborted) return reject(new DOMException("Aborted", "AbortError"));
        const timer = setTimeout(resolve, ms);
        signal?.addEventListener("abort", () => {
            clearTimeout(timer);
            reject(new DOMException("Aborted", "AbortError"));
        }, { once: true });
    });
}
