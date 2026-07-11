import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";

const management = await readFile(new URL("./McpServersSettings.svelte", import.meta.url), "utf8");
const drawer = await readFile(new URL("./McpDrawer.svelte", import.meta.url), "utf8");
const settings = await readFile(new URL("./Settings.svelte", import.meta.url), "utf8");
const integrations = await readFile(new URL("./Integrations.svelte", import.meta.url), "utf8");
const statusModel = await readFile(new URL("../services/mcpOAuthPolling.ts", import.meta.url), "utf8");
const service = await readFile(new URL("../services/mcpServerService.svelte.ts", import.meta.url), "utf8");

for (const stage of ["Connection", "Authentication", "Ready"]) assert.match(management, new RegExp(`\\b${stage}\\b`));
for (const status of ["Connected", "Sign-in required", "Waiting for authorization", "Connection error", "Disabled"]) {
  assert.match(management + drawer + statusModel, new RegExp(status));
}
for (const action of ["Sign in", "Retry", "Disconnect", "Enable", "Edit", "Remove", "Use in new chat"]) {
  assert.match(management, new RegExp(action));
}

assert.match(management, /Remote URL/);
assert.match(management, /Local command/);
assert.match(management, /No authentication/);
assert.match(management, /OAuth/);
assert.match(management, /Advanced: static bearer token/);
assert.match(management, /confirm\(`/);
assert.match(management, /openAuthorization\(session\)/);
assert.match(service, /opener: \(url: string\) => Window \| null/);
assert.match(service, /return Boolean\(opener\(session\.authorizationUrl\)\)/);
assert.match(management, /popupFailed/);
assert.match(management, /Open authorization/);
assert.match(management, /Copy link/);
assert.match(management, /role="alert"/);
assert.match(management, /role="status"/);
assert.match(management, /aria-live="polite"/);
assert.match(management, /focus-visible:ring-2/);
assert.match(management, /xl:grid-cols-2/);
assert.match(management, /sm:flex-row/);
assert.match(management, /denied/);
assert.match(management, /expired/);

assert.match(drawer, /MCP tools for this chat/);
assert.match(drawer, /disabled=\{!\$isFirstMessage\}/);
assert.match(drawer, /Manage MCP servers/);
assert.match(drawer, /\$settingsSection = "connections"/);
assert.doesNotMatch(drawer, /createServer|deleteServer|setToolEnabled|Bearer token|Add MCP server/);

assert.match(settings, /Settings → Connections/);
assert.match(settings, /<McpServersSettings \/>/);
assert.match(settings, /<Integrations embedded \/>/);
assert.doesNotMatch(integrations, /MCP servers|mcpServerService|createServer|deleteServer/);

const snapshotDirectory = new URL("./__snapshots__/", import.meta.url);
const snapshotFiles = (await readdir(snapshotDirectory)).filter((name) => name.endsWith(".jpg"));
assert.deepEqual(snapshotFiles.sort(), ["mcp-settings-narrow.jpg", "mcp-settings-normal.jpg"]);
for (const name of snapshotFiles) {
  const artifact = (await readFile(new URL(name, snapshotDirectory))).toString("latin1");
  for (const secret of ["fixture-static-bearer", "access_token", "refresh_token", "client_secret", "code_verifier", "authorization-code", "raw-state"]) {
    assert(!artifact.includes(secret), `${name} contains secret material: ${secret}`);
  }
}

console.log("MCP management component contract tests passed");
