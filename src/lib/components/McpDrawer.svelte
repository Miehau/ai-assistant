<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronLeft, Plus, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { mcpServerService } from "$lib/services/mcpServerService.svelte";
  import { loadMcpServers } from "$lib/stores/chat";
  import type { McpServer } from "$lib/types/mcpServer";

  export let isOpen = false;
  export let onBack: (() => void) | undefined = undefined;

  let name = "";
  let transport: "stdio" | "streamable_http" = "stdio";
  let command = "npx";
  let argsText = "@playwright/mcp@latest";
  let url = "";
  let bearerToken = "";
  let isAdding = false;
  let actionError = "";

  onMount(() => {
    void refreshMcpServers();
  });

  $: if (isOpen) {
    void refreshMcpServers();
  }

  async function refreshMcpServers() {
    await loadMcpServers();
  }

  function closeDrawer() {
    isOpen = false;
    onBack?.();
  }

  function usePlaywrightPreset() {
    name = "playwright";
    transport = "stdio";
    command = "npx";
    argsText = "@playwright/mcp@latest";
    url = "";
    bearerToken = "";
  }

  function parseArgs(value: string): string[] {
    return value.split(/\s+/).map((part) => part.trim()).filter(Boolean);
  }

  async function addServer() {
    if (!name.trim()) return;
    actionError = "";
    isAdding = true;
    try {
      await mcpServerService.createServer({
        name: name.trim(),
        transport,
        command: transport === "stdio" ? command.trim() : null,
        args: transport === "stdio" ? parseArgs(argsText) : [],
        url: transport === "streamable_http" ? url.trim() : null,
        bearerToken: transport === "streamable_http" ? bearerToken.trim() || null : null,
        enabled: true
      });
      name = "";
      command = "npx";
      argsText = "@playwright/mcp@latest";
      url = "";
      bearerToken = "";
      await refreshMcpServers();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      isAdding = false;
    }
  }

  async function setServerEnabled(server: McpServer, enabled: boolean) {
    actionError = "";
    try {
      await mcpServerService.updateServer({ id: server.id, enabled });
      await refreshMcpServers();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    }
  }

  async function deleteServer(server: McpServer) {
    if (!confirm(`Delete ${server.name}?`)) return;
    actionError = "";
    try {
      await mcpServerService.deleteServer(server.id);
      await refreshMcpServers();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    }
  }

  async function toggleTool(server: McpServer, toolName: string, enabled: boolean) {
    actionError = "";
    try {
      await mcpServerService.setToolEnabled(server.id, toolName, enabled);
      await refreshMcpServers();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    }
  }
</script>

<div
  class="fixed inset-0 z-40 bg-black/45 drawer-overlay"
  class:drawer-overlay-open={isOpen}
  inert={!isOpen}
  onclick={(e) => { e.stopPropagation(); isOpen = false; }}
  role="button"
  tabindex={isOpen ? 0 : -1}
  aria-hidden={!isOpen}
  onkeydown={(e) => { if (e.key === "Escape") isOpen = false; }}
></div>
<div
  class="fixed top-8 bottom-0 left-0 z-50 w-[75vw] max-w-[75vw] glass-panel-minimal rounded-r-2xl border-r border-white/10 shadow-2xl flex flex-col overflow-hidden drawer-panel"
  class:drawer-panel-open={isOpen}
  inert={!isOpen}
  aria-hidden={!isOpen}
>
  <div class="flex items-center justify-between border-b border-white/10 px-5 py-4 shrink-0">
    <div>
      <span class="text-[11px] uppercase tracking-wide text-muted-foreground/70">MCP Servers</span>
      <p class="text-xs text-muted-foreground/70">Manage servers and tools for new chat sessions.</p>
    </div>
    <Button variant="ghost" size="icon" class="rounded-lg" onclick={closeDrawer} aria-label="Back">
      <ChevronLeft class="size-4" />
    </Button>
  </div>

  <div class="flex-1 overflow-y-auto p-5 space-y-5">
    <section class="grid gap-3 rounded-lg border border-white/10 bg-white/5 p-4">
      <div class="flex items-center justify-between gap-3">
        <h3 class="text-sm font-semibold">Add MCP server</h3>
        <Button variant="outline" size="sm" onclick={usePlaywrightPreset}>Playwright preset</Button>
      </div>
      <div class="grid gap-3 sm:grid-cols-2">
        <label class="grid gap-1 text-xs">
          <span class="text-muted-foreground">Name</span>
          <Input bind:value={name} placeholder="playwright" class="h-8 bg-white/5 border-white/10" />
        </label>
        <label class="grid gap-1 text-xs">
          <span class="text-muted-foreground">Transport</span>
          <select bind:value={transport} class="h-8 rounded-md border border-white/10 bg-background px-2 text-xs">
            <option value="stdio">Local stdio</option>
            <option value="streamable_http">Streamable HTTP</option>
          </select>
        </label>
        {#if transport === "stdio"}
          <label class="grid gap-1 text-xs">
            <span class="text-muted-foreground">Command</span>
            <Input bind:value={command} placeholder="npx" class="h-8 bg-white/5 border-white/10" />
          </label>
          <label class="grid gap-1 text-xs">
            <span class="text-muted-foreground">Args</span>
            <Input bind:value={argsText} placeholder="@playwright/mcp@latest" class="h-8 bg-white/5 border-white/10" />
          </label>
        {:else}
          <label class="grid gap-1 text-xs">
            <span class="text-muted-foreground">URL</span>
            <Input bind:value={url} placeholder="https://example.com/mcp" class="h-8 bg-white/5 border-white/10" />
          </label>
          <label class="grid gap-1 text-xs">
            <span class="text-muted-foreground">Bearer token</span>
            <Input bind:value={bearerToken} type="password" placeholder="Optional" class="h-8 bg-white/5 border-white/10" />
          </label>
        {/if}
      </div>
      <div class="flex items-center gap-2">
        <Button size="sm" onclick={addServer} disabled={isAdding}>
          <Plus class="h-4 w-4 mr-1" />
          {isAdding ? "Adding..." : "Add and enable"}
        </Button>
        {#if actionError}
          <p class="text-xs text-red-400">{actionError}</p>
        {/if}
      </div>
    </section>

    <section class="space-y-3">
      {#if mcpServerService.loading}
        <p class="text-sm text-muted-foreground">Loading MCP servers...</p>
      {:else if mcpServerService.servers.length === 0}
        <p class="text-sm text-muted-foreground">No MCP servers configured.</p>
      {:else}
        {#each mcpServerService.servers as server (server.id)}
          <div class="rounded-lg border border-white/10 bg-white/5 p-4">
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="flex flex-wrap items-center gap-2">
                  <h3 class="text-sm font-semibold">{server.name}</h3>
                  <span class="rounded-full px-2 py-0.5 text-[10px] uppercase bg-white/10 text-muted-foreground">{server.transport}</span>
                  <span class={`rounded-full px-2 py-0.5 text-[10px] uppercase ${server.status === "connected" ? "bg-emerald-500/15 text-emerald-300" : server.status === "error" ? "bg-red-500/15 text-red-300" : "bg-white/10 text-muted-foreground"}`}>{server.status}</span>
                </div>
                <p class="mt-1 truncate text-xs font-mono text-muted-foreground">
                  {server.transport === "stdio" ? `${server.command ?? ""} ${server.args.join(" ")}` : server.url}
                </p>
                {#if server.error}
                  <p class="mt-1 text-xs text-red-400">{server.error}</p>
                {/if}
              </div>
              <div class="flex shrink-0 items-center gap-2">
                <label class="flex items-center gap-2 text-xs">
                  <input type="checkbox" checked={server.enabled} onchange={(event) => setServerEnabled(server, event.currentTarget.checked)} />
                  Enabled
                </label>
                <Button variant="ghost" size="icon" class="h-8 w-8 text-destructive" onclick={() => deleteServer(server)}>
                  <Trash2 class="h-4 w-4" />
                </Button>
              </div>
            </div>

            <div class="mt-3 space-y-1">
              {#if server.tools.length === 0}
                <p class="text-xs text-muted-foreground">No tools discovered yet.</p>
              {:else}
                {#each server.tools as tool (tool.id)}
                  <label class="flex items-start justify-between gap-3 rounded-md border border-white/10 bg-background/30 px-3 py-2 text-xs">
                    <span class="min-w-0">
                      <span class="block truncate font-medium">{tool.remoteName}</span>
                      <span class="block truncate text-[11px] text-muted-foreground">{tool.description || tool.registeredName}</span>
                    </span>
                    <span class="flex shrink-0 items-center gap-2">
                      <span class="text-[10px] text-muted-foreground">New sessions</span>
                      <input
                        type="checkbox"
                        checked={tool.enabledForNewSessions}
                        onchange={(event) => toggleTool(server, tool.remoteName, event.currentTarget.checked)}
                      />
                    </span>
                  </label>
                {/each}
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </section>
  </div>
</div>
