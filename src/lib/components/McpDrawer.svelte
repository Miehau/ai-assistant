<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronLeft, Settings2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { mcpServerService } from "$lib/services/mcpServerService.svelte";
  import { isFirstMessage, selectedMcpServerIds } from "$lib/stores/chat";
  import { settingsDrawerOpen, settingsSection } from "$lib/stores/drawers";

  export let isOpen = false;
  export let onBack: (() => void) | undefined = undefined;

  onMount(() => {
    void mcpServerService.loadServers();
  });

  $: if (isOpen) void mcpServerService.loadServers();

  function closeDrawer() {
    isOpen = false;
    onBack?.();
  }

  function toggleServer(serverId: string, selected: boolean) {
    if (!$isFirstMessage) return;
    selectedMcpServerIds.update((ids) =>
      selected ? [...new Set([...ids, serverId])] : ids.filter((id) => id !== serverId),
    );
  }

  function manageServers() {
    isOpen = false;
    $settingsSection = "connections";
    $settingsDrawerOpen = true;
  }
</script>

<div
  class="fixed inset-0 z-40 bg-black/45 drawer-overlay"
  class:drawer-overlay-open={isOpen}
  inert={!isOpen}
  onclick={(event) => { if (event.currentTarget === event.target) closeDrawer(); }}
  aria-hidden={!isOpen}
></div>
<aside
  class="fixed top-8 bottom-0 left-0 z-50 flex w-[min(92vw,28rem)] flex-col overflow-hidden rounded-r-2xl border-r border-white/10 shadow-2xl glass-panel-minimal drawer-panel"
  class:drawer-panel-open={isOpen}
  inert={!isOpen}
  aria-hidden={!isOpen}
  aria-labelledby="mcp-drawer-heading"
  onkeydown={(event) => { if (event.key === "Escape") closeDrawer(); }}
>
  <header class="flex shrink-0 items-center justify-between border-b border-white/10 px-5 py-4">
    <div>
      <h2 id="mcp-drawer-heading" class="text-sm font-semibold">MCP tools for this chat</h2>
      <p class="text-xs text-muted-foreground/70">Choose connected servers before sending the first message.</p>
    </div>
    <Button variant="ghost" size="icon" class="rounded-lg" onclick={closeDrawer} aria-label="Back">
      <ChevronLeft class="size-4" />
    </Button>
  </header>

  <div class="flex-1 space-y-4 overflow-y-auto p-5">
    {#if !$isFirstMessage}
      <p class="rounded-lg border border-amber-400/20 bg-amber-500/10 p-3 text-xs text-amber-100" role="status">
        MCP server selection is locked after a chat starts. Start a new chat to change it.
      </p>
    {/if}

    {#if mcpServerService.loading}
      <p class="text-sm text-muted-foreground" aria-live="polite">Loading MCP servers…</p>
    {:else if mcpServerService.servers.filter((server) => server.enabled && server.connectionStatus === "connected").length === 0}
      <p class="rounded-lg border border-dashed border-white/15 p-4 text-sm text-muted-foreground">
        No connected MCP servers are available. Add or reconnect one in Settings.
      </p>
    {:else}
      <fieldset class="space-y-2" disabled={!$isFirstMessage}>
        <legend class="sr-only">Connected MCP servers</legend>
        {#each mcpServerService.servers.filter((server) => server.enabled && server.connectionStatus === "connected") as server (server.id)}
          <label class="flex items-start gap-3 rounded-lg border border-white/10 bg-white/5 p-3 text-sm">
            <input
              class="mt-1"
              type="checkbox"
              checked={$selectedMcpServerIds.includes(server.id)}
              onchange={(event) => toggleServer(server.id, event.currentTarget.checked)}
            />
            <span class="min-w-0">
              <span class="block font-medium">{server.name}</span>
              <span class="block text-xs text-muted-foreground">{server.tools.filter((tool) => tool.enabledForNewSessions).length} enabled tools</span>
            </span>
          </label>
        {/each}
      </fieldset>
    {/if}

    <Button class="w-full" variant="outline" onclick={manageServers}>
      <Settings2 class="mr-2 size-4" />Manage MCP servers
    </Button>
  </div>
</aside>
