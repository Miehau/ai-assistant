import assert from "node:assert/strict";
import { HttpBackendClient, HttpBackendError } from "../backend/http-client";
import type { McpOAuthSession, McpServer } from "../types/mcpServer";
import {
    mcpStatusLabel,
    normalizeMcpHttpError,
    pendingMcpServerIds,
    pollMcpOAuthSession,
} from "./mcpOAuthPolling";

const pending = session("pending");
const authorized = session("authorized");
let now = 0;
let calls = 0;
const terminal = await pollMcpOAuthSession(async () => (++calls === 1 ? pending : authorized), {
    intervalMs: 10,
    timeoutMs: 100,
    now: () => now,
    sleep: async (ms) => { now += ms; },
});
assert.equal(terminal.reason, "terminal");
assert.equal(terminal.session?.status, "authorized");
assert.equal(calls, 2);

now = 0;
calls = 0;
const timeout = await pollMcpOAuthSession(async () => { calls++; return pending; }, {
    intervalMs: 25,
    timeoutMs: 50,
    now: () => now,
    sleep: async (ms) => { now += ms; },
});
assert.equal(timeout.reason, "timeout");
assert.equal(calls, 3);

const cancelledController = new AbortController();
const cancelled = await pollMcpOAuthSession(async () => pending, {
    signal: cancelledController.signal,
    sleep: async () => {
        cancelledController.abort();
        throw new DOMException("Aborted", "AbortError");
    },
});
assert.equal(cancelled.reason, "cancelled");

for (const status of ["authorized", "denied", "expired", "cancelled", "error"] as const) {
    const outcome = await pollMcpOAuthSession(async () => session(status));
    assert.equal(outcome.reason, "terminal");
    assert.equal(outcome.session?.status, status);
}
assert.equal((await pollMcpOAuthSession(async () => null)).reason, "terminal");

assert.deepEqual([
    mcpStatusLabel({ authStatus: "not_required", connectionStatus: "disabled" }),
    mcpStatusLabel({ authStatus: "pending", connectionStatus: "connecting" }),
    mcpStatusLabel({ authStatus: "required", connectionStatus: "error" }),
    mcpStatusLabel({ authStatus: "authorized", connectionStatus: "connected" }),
    mcpStatusLabel({ authStatus: "error", connectionStatus: "error" }),
], ["Disabled", "Waiting for authorization", "Sign-in required", "Connected", "Connection error"]);

const servers = [
    serverFixture("pending-live", pending),
    serverFixture("pending-expired", { ...pending, expiresAt: 9 }),
    serverFixture("ready", authorized),
];
assert.deepEqual(pendingMcpServerIds(servers, 10), ["pending-live"]);

for (const server of [
    { ...serverFixture("no-auth", null), authMode: "none" as const, authStatus: "not_required" as const, connectionStatus: "connected" as const, status: "connected" as const },
    { ...serverFixture("bearer", null), authMode: "bearer" as const, bearerTokenConfigured: true, authStatus: "not_required" as const, connectionStatus: "connected" as const, status: "connected" as const },
    { ...serverFixture("stdio", null), transport: "stdio" as const, command: "node", url: "", authMode: "none" as const, authStatus: "not_required" as const, connectionStatus: "connected" as const, status: "connected" as const },
]) {
    assert.equal(mcpStatusLabel(server), "Connected");
    assertNoSecretFields(server);
}

assert.match(normalizeMcpHttpError(new HttpBackendError("Unauthorized", 401)), /authentication expired/);
assert.match(normalizeMcpHttpError(new HttpBackendError("missing", 404)), /another user/);
assert.match(normalizeMcpHttpError(new HttpBackendError("conflict", 409)), /no longer available/);
assert.match(normalizeMcpHttpError(new HttpBackendError("PUBLIC_BASE_URL bad", 400)), /callback configuration/);
assert.match(normalizeMcpHttpError(new HttpBackendError("ENCRYPTION_KEY missing", 400)), /credential storage/);
assert.match(normalizeMcpHttpError(new HttpBackendError("limited", 429)), /Too many/);
assert.match(normalizeMcpHttpError(new Error("raw provider body token=do-not-echo")), /Could not update/);

const requests: Array<{ url: string; method: string; authorization: string | null; body?: string }> = [];
const originalFetch = globalThis.fetch;
globalThis.fetch = async (input, init) => {
    const url = String(input);
    requests.push({
        url,
        method: init?.method ?? "GET",
        authorization: new Headers(init?.headers).get("Authorization"),
        body: typeof init?.body === "string" ? init.body : undefined,
    });
    const body = url.endsWith("/oauth/start")
        ? { session: { ...pending, authorizationUrl: "https://auth.test/authorize?fixture=1" } }
        : url.endsWith("/oauth/session")
            ? { session: authorized }
            : serverFixture("server", authorized);
    return new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } });
};

try {
    const client = new HttpBackendClient({ serverUrl: "https://backend.test/", token: "application-token" });
    await client.connectMcpServer("server");
    await client.startMcpOAuth("server");
    await client.getMcpOAuthSession("server");
    await client.cancelMcpOAuth("server");
    await client.disconnectMcpServer("server");
    await client.reconnectMcpServer("server");
    assert.deepEqual(requests.map((request) => [request.method, new URL(request.url).pathname]), [
        ["POST", "/api/mcps/server/connect"],
        ["POST", "/api/mcps/server/oauth/start"],
        ["GET", "/api/mcps/server/oauth/session"],
        ["POST", "/api/mcps/server/oauth/cancel"],
        ["POST", "/api/mcps/server/disconnect"],
        ["POST", "/api/mcps/server/reconnect"],
    ]);
    assert(requests.every((request) => request.authorization === "Bearer application-token"));
    assert(requests.every((request) => request.body === undefined));
} finally {
    globalThis.fetch = originalFetch;
}

assertNoSecretFields(serverFixture("safe", authorized));
assertNoSecretFields(pending);
console.log("MCP frontend polling and HTTP contract tests passed");

function session(status: McpOAuthSession["status"]): McpOAuthSession {
    return { id: `session-${status}`, serverId: "server", status, expiresAt: status === "pending" ? 100 : 0, error: null };
}

function serverFixture(id: string, oauthSession: McpOAuthSession | null): McpServer {
    return {
        id, name: id, transport: "streamable_http", command: "", args: [], env: {}, cwd: null, url: "https://mcp.test/mcp",
        bearerTokenConfigured: false, authMode: "oauth", authStatus: oauthSession?.status === "pending" ? "pending" : "authorized",
        connectionStatus: oauthSession?.status === "authorized" ? "connected" : "connecting", oauthCredentialsConfigured: oauthSession?.status === "authorized",
        oauthSession, enabled: true, status: oauthSession?.status === "authorized" ? "connected" : "connecting", error: null,
        createdAt: 1, updatedAt: 1, tools: [],
    };
}

function assertNoSecretFields(value: unknown): void {
    const json = JSON.stringify(value);
    for (const field of ["access_token", "refresh_token", "client_secret", "code_verifier", "stateHash", "state_hash"]) {
        assert(!json.includes(field));
    }
}

function compileTimeSecretBoundary(server: McpServer, oauthSession: McpOAuthSession): void {
    // @ts-expect-error frontend records deliberately have no access token
    void server.access_token;
    // @ts-expect-error frontend records deliberately have no refresh token
    void server.refresh_token;
    // @ts-expect-error frontend OAuth sessions deliberately have no verifier
    void oauthSession.code_verifier;
    // @ts-expect-error frontend OAuth sessions deliberately have no raw state
    void oauthSession.state;
}
void compileTimeSecretBoundary;
