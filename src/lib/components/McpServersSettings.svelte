<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { get } from "svelte/store";
  import {
    AlertCircle, CheckCircle2, Copy, ExternalLink, Globe2, LoaderCircle,
    LogOut, Pencil, Plus, RotateCw, TerminalSquare, Trash2,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { mcpServerService, type McpPollingHandle } from "$lib/services/mcpServerService.svelte";
  import { mcpStatusLabel } from "$lib/services/mcpOAuthPolling";
  import { isFirstMessage, selectedMcpServerIds } from "$lib/stores/chat";
  import type { McpAuthMode, McpOAuthSession, McpServer, McpTransport } from "$lib/types/mcpServer";

  let stage = $state<1 | 2 | 3>(1);
  let transport = $state<McpTransport>("streamable_http");
  let name = $state("");
  let nameEdited = $state(false);
  let url = $state("");
  let command = $state("npx");
  let argsText = $state("");
  let authMode = $state<McpAuthMode>("auto");
  let bearerToken = $state("");
  let advancedOpen = $state(false);
  let createdServerId = $state<string | null>(null);
  let popupFailed = $state(false);
  let fallbackUrl = $state<string | null>(null);
  let busy = $state(false);
  let error = $state("");
  let notice = $state("");
  let editingId = $state<string | null>(null);
  let editName = $state("");
  let editLocation = $state("");
  let editAuthMode = $state<McpAuthMode>("auto");
  let editBearerToken = $state("");
  let readyHeading = $state<HTMLHeadingElement>();
  let errorBox = $state<HTMLDivElement>();
  let polling: McpPollingHandle | null = null;
  const fieldClass = "border-white/25 bg-black/30 shadow-inner focus-visible:ring-2 focus-visible:ring-emerald-400/70";

  const createdServer = $derived(
    createdServerId ? mcpServerService.servers.find((server) => server.id === createdServerId) ?? null : null,
  );

  $effect(() => {
    if (nameEdited || name.trim()) return;
    name = inferName(transport === "stdio" ? command : url);
  });

  onMount(async () => {
    await mcpServerService.restorePendingOAuth();
  });

  onDestroy(() => {
    polling?.cancel();
    mcpServerService.dispose();
  });

  function parseArgs(value: string): string[] {
    return value.match(/(?:[^\s"]+|"[^"]*")+/g)?.map((part) => part.replace(/^"|"$/g, "")) ?? [];
  }

  function connectionValid(): boolean {
    return Boolean(name.trim() && (transport === "stdio" ? command.trim() : validHttpUrl(url)));
  }

  function continueToAuthentication() {
    error = "";
    if (!connectionValid()) {
      showError(transport === "stdio" ? "Enter a name and local command." : "Enter a name and valid HTTP or HTTPS MCP URL.");
      return;
    }
    if (transport === "stdio") authMode = "none";
    stage = 2;
  }

  async function createAndConnect() {
    busy = true;
    error = "";
    notice = "";
    try {
      const server = await mcpServerService.createServer({
        name: name.trim(), transport,
        command: transport === "stdio" ? command.trim() : null,
        args: transport === "stdio" ? parseArgs(argsText) : [],
        url: transport === "streamable_http" ? url.trim() : null,
        authMode: transport === "stdio" ? "none" : authMode,
        bearerToken: authMode === "bearer" ? bearerToken.trim() || null : null,
        enabled: false,
      });
      createdServerId = server.id;
      const connected = await mcpServerService.connect(server.id);
      if (connected.connectionStatus === "connected") await showReady();
      else if (connected.authStatus === "required" || connected.authStatus === "pending") {
        notice = "This server requires sign-in before tools can be discovered.";
      } else {
        showError(connected.error || "The server could not connect. Check its address and try again.");
      }
    } catch (cause) {
      showError(cause instanceof Error ? cause.message : "The server could not be added.");
    } finally {
      busy = false;
    }
  }

  async function beginSignIn(serverId: string) {
    busy = true;
    error = "";
    notice = "";
    popupFailed = false;
    try {
      const session = await mcpServerService.startOAuth(serverId);
      fallbackUrl = session.authorizationUrl;
      popupFailed = !mcpServerService.openAuthorization(session);
      notice = popupFailed
        ? "The browser did not open. Use the secure authorization link below."
        : "Waiting for authorization. Complete sign-in in your browser, then return here.";
      polling?.cancel();
      polling = mcpServerService.pollOAuth(serverId);
      const outcome = await polling.promise;
      const server = await mcpServerService.getServer(serverId);
      if (outcome.reason === "timeout") showError("Authorization timed out. Start sign-in again.");
      else if (outcome.reason === "cancelled") return;
      else if (server?.connectionStatus === "connected") await showReady(serverId);
      else showError(sessionMessage(outcome.session));
    } catch (cause) {
      showError(cause instanceof Error ? cause.message : "Authorization could not be started.");
    } finally {
      busy = false;
    }
  }

  async function showReady(serverId = createdServerId) {
    if (serverId) createdServerId = serverId;
    stage = 3;
    notice = "Connected and ready for new chats.";
    await tick();
    readyHeading?.focus();
  }

  function resetWizard() {
    polling?.cancel(); polling = null;
    stage = 1; transport = "streamable_http"; name = ""; nameEdited = false; url = "";
    command = "npx"; argsText = ""; authMode = "auto"; bearerToken = ""; advancedOpen = false;
    createdServerId = null; popupFailed = false; fallbackUrl = null; error = ""; notice = "";
  }

  async function copyFallback() {
    if (!fallbackUrl) return;
    try { await navigator.clipboard.writeText(fallbackUrl); notice = "Authorization link copied."; }
    catch { showError("Could not copy the link. Open it directly instead."); }
  }

  async function runAction(action: () => Promise<unknown>) {
    busy = true; error = ""; notice = "";
    try { await action(); }
    catch (cause) { showError(cause instanceof Error ? cause.message : "The connection could not be updated."); }
    finally { busy = false; }
  }

  function startEdit(server: McpServer) {
    editingId = server.id; editName = server.name;
    editLocation = server.transport === "stdio" ? `${server.command} ${server.args.join(" ")}`.trim() : server.url;
    editAuthMode = server.authMode; editBearerToken = "";
  }

  async function saveEdit(server: McpServer) {
    await runAction(async () => {
      await mcpServerService.updateServer({
        id: server.id, name: editName.trim(), authMode: editAuthMode,
        ...(server.transport === "stdio"
          ? { command: parseArgs(editLocation)[0] ?? "", args: parseArgs(editLocation).slice(1) }
          : { url: editLocation.trim(), bearerToken: editAuthMode === "bearer" ? editBearerToken.trim() || undefined : null }),
      });
      editingId = null;
    });
  }

  async function removeServer(server: McpServer) {
    if (!confirm(`Remove ${server.name} and its discovered tools?`)) return;
    await runAction(() => mcpServerService.deleteServer(server.id));
  }

  function useInNewChat(server: McpServer) {
    if (!get(isFirstMessage)) return;
    selectedMcpServerIds.update((ids) => ids.includes(server.id) ? ids : [...ids, server.id]);
    notice = `${server.name} will be available in the new chat.`;
  }

  function showError(message: string) {
    error = message;
    void tick().then(() => errorBox?.focus());
  }

  function sessionMessage(session: McpOAuthSession | null): string {
    if (session?.status === "denied") return "Authorization was denied. Choose Sign in to try again.";
    if (session?.status === "expired") return "Authorization expired. Choose Sign in to try again.";
    if (session?.status === "cancelled") return "Authorization was cancelled.";
    return session?.error || "Authorization could not be completed. Try again.";
  }

  function inferName(value: string): string {
    const trimmed = value.trim();
    if (!trimmed) return "";
    try {
      const parsed = new URL(trimmed);
      const path = parsed.pathname.split("/").filter(Boolean).at(-1);
      return (path && path !== "mcp" ? path : parsed.hostname.split(".")[0]).replace(/[-_]+/g, " ");
    } catch { return trimmed.split(/[/\\\s]/).filter(Boolean).at(-1)?.replace(/[-_]+/g, " ") ?? ""; }
  }

  function validHttpUrl(value: string): boolean {
    try { return ["http:", "https:"].includes(new URL(value).protocol); } catch { return false; }
  }
</script>

<section class="mt-6 space-y-4" aria-labelledby="mcp-settings-heading">
  <header>
    <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">MCP servers</p>
    <h3 id="mcp-settings-heading" class="text-sm font-semibold">Model Context Protocol connections</h3>
    <p class="mt-1 text-[11px] text-muted-foreground/70">Add remote OAuth or bearer servers and local stdio commands. Credentials stay in the backend.</p>
  </header>

  <div class="rounded-xl border border-emerald-400/20 bg-emerald-500/[0.04] p-4" aria-labelledby="setup-heading">
    <div class="mb-4 flex flex-wrap items-center justify-between gap-2">
      <div>
        <p class="text-[10px] uppercase tracking-wide text-muted-foreground">Step {stage} of 3</p>
        <h4 id="setup-heading" class="font-semibold">{stage === 1 ? "Connection" : stage === 2 ? "Authentication" : "Ready"}</h4>
      </div>
      {#if stage !== 1}<Button variant="ghost" size="sm" onclick={resetWizard}>Start over</Button>{/if}
    </div>

    {#if error}
      <div bind:this={errorBox} tabindex="-1" role="alert" class="mb-3 flex gap-2 rounded-lg border border-red-400/40 bg-red-500/10 p-3 text-xs text-red-200">
        <AlertCircle class="mt-0.5 size-4 shrink-0" /> <span>{error}</span>
      </div>
    {/if}
    {#if notice}<p role="status" aria-live="polite" class="mb-3 text-xs text-muted-foreground">{notice}</p>{/if}

    {#if stage === 1}
      <fieldset class="grid gap-3"><legend class="sr-only">MCP connection details</legend>
        <div class="grid gap-2 xl:grid-cols-2">
          <label class="grid gap-1 text-xs" for="mcp-transport"><span>Connection type</span>
            <select id="mcp-transport" bind:value={transport} class="h-9 rounded-md border border-white/15 bg-background px-3 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
              <option value="streamable_http">Remote URL</option><option value="stdio">Local command</option>
            </select>
          </label>
          {#if transport === "streamable_http"}
            <label class="grid gap-1 text-xs" for="mcp-url"><span>Streamable HTTP URL</span><Input id="mcp-url" class={fieldClass} type="url" bind:value={url} placeholder="https://example.com/mcp" /></label>
          {:else}
            <label class="grid gap-1 text-xs" for="mcp-command"><span>Command</span><Input id="mcp-command" class={fieldClass} bind:value={command} placeholder="npx" /></label>
          {/if}
        </div>
        <div class="grid gap-2 xl:grid-cols-2">
          <label class="grid gap-1 text-xs" for="mcp-name"><span>Display name</span>
            <Input id="mcp-name" class={fieldClass} bind:value={name} placeholder="My MCP server" oninput={() => nameEdited = true} aria-describedby="mcp-name-help" />
            <span id="mcp-name-help" class="text-[10px] text-muted-foreground">Inferred from the address; edit it any time.</span>
          </label>
          {#if transport === "stdio"}
            <label class="grid gap-1 text-xs" for="mcp-args"><span>Arguments</span><Input id="mcp-args" class={fieldClass} bind:value={argsText} placeholder="@playwright/mcp@latest" /></label>
          {/if}
        </div>
      </fieldset>
      <div class="mt-4"><Button onclick={continueToAuthentication} disabled={!connectionValid()}>Continue</Button></div>
    {:else if stage === 2}
      {#if !createdServerId}
        <fieldset class="space-y-3"><legend class="text-xs font-medium">How should this server authenticate?</legend>
          {#if transport === "stdio"}
            <p class="text-xs text-muted-foreground"><TerminalSquare class="mr-1 inline size-4" />Local stdio servers receive credentials through their configured environment.</p>
          {:else}
            <label class="flex gap-2 text-xs"><input type="radio" bind:group={authMode} value="auto" /> <span><b>Detect automatically</b><br><span class="text-muted-foreground">Try no authentication, then OAuth when required.</span></span></label>
            <label class="flex gap-2 text-xs"><input type="radio" bind:group={authMode} value="none" /> <span><b>No authentication</b></span></label>
            <label class="flex gap-2 text-xs"><input type="radio" bind:group={authMode} value="oauth" /> <span><b>OAuth</b><br><span class="text-muted-foreground">Sign in in your browser without exposing credentials here.</span></span></label>
            <details bind:open={advancedOpen} class="rounded-lg border border-white/10 p-3"><summary class="cursor-pointer text-xs font-medium">Advanced: static bearer token</summary>
              <label class="mt-3 flex gap-2 text-xs"><input type="radio" bind:group={authMode} value="bearer" /> Use a static bearer token</label>
              {#if authMode === "bearer"}<label class="mt-2 grid gap-1 text-xs" for="mcp-bearer"><span>Bearer token</span><Input id="mcp-bearer" class={fieldClass} type="password" autocomplete="off" bind:value={bearerToken} /></label>{/if}
            </details>
          {/if}
        </fieldset>
        <div class="mt-4 flex gap-2"><Button variant="outline" onclick={() => stage = 1}>Back</Button><Button onclick={createAndConnect} disabled={busy}>{busy ? "Connecting…" : "Connect"}</Button></div>
      {:else if createdServer?.connectionStatus === "connected"}
        <Button onclick={() => showReady()}>Continue to ready</Button>
      {:else}
        <div class="rounded-lg border border-white/10 p-3 text-xs">
          <p class="font-medium">Sign in to continue</p><p class="mt-1 text-muted-foreground">OAuth happens in your browser. This app receives status only.</p>
          <Button class="mt-3" onclick={() => beginSignIn(createdServerId!)} disabled={busy}>{busy ? "Waiting…" : "Sign in"}</Button>
        </div>
      {/if}
      {#if fallbackUrl && popupFailed}
        <div class="mt-3 rounded-lg border border-amber-400/40 bg-amber-500/10 p-3 text-xs" role="status">
          <p>The browser popup was blocked.</p><div class="mt-2 flex flex-wrap gap-2">
            <a class="inline-flex items-center gap-1 underline focus-visible:ring-2" href={fallbackUrl} target="_blank" rel="noopener noreferrer"><ExternalLink class="size-3" />Open authorization</a>
            <button class="inline-flex items-center gap-1 underline focus-visible:ring-2" type="button" onclick={copyFallback}><Copy class="size-3" />Copy link</button>
          </div>
        </div>
      {/if}
    {:else}
      <div class="space-y-3">
        <div class="flex gap-2"><CheckCircle2 class="size-5 text-emerald-300" /><div><h5 bind:this={readyHeading} tabindex="-1" class="font-semibold">Ready</h5><p class="text-xs text-muted-foreground">{createdServer?.tools.length ?? 0} tools discovered.</p></div></div>
        {#if createdServer}
          <div class="space-y-1">{#each createdServer.tools as tool}<label class="flex justify-between gap-3 rounded border border-white/10 p-2 text-xs"><span>{tool.remoteName}</span><input type="checkbox" checked={tool.enabledForNewSessions} onchange={(event) => mcpServerService.setToolEnabled(createdServer.id, tool.remoteName, event.currentTarget.checked)} /></label>{/each}</div>
          <div class="flex flex-wrap gap-2"><Button onclick={() => useInNewChat(createdServer)} disabled={!$isFirstMessage}>Use in new chat</Button><Button variant="outline" onclick={resetWizard}>Add another</Button></div>
          {#if !$isFirstMessage}<p class="text-xs text-muted-foreground">MCP selection is locked after a chat starts.</p>{/if}
        {/if}
      </div>
    {/if}
  </div>

  <div class="space-y-3" aria-live="polite">
    {#if mcpServerService.loading}<p class="text-xs text-muted-foreground">Loading MCP servers…</p>
    {:else if mcpServerService.servers.length === 0}<p class="rounded-lg border border-dashed border-white/15 p-4 text-xs text-muted-foreground">No MCP servers configured yet.</p>
    {:else}
      {#each mcpServerService.servers as server (server.id)}
        <article class="rounded-xl border border-white/10 bg-white/5 p-4" aria-labelledby={`server-${server.id}`}>
          <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div class="min-w-0">
              <div class="flex flex-wrap items-center gap-2"><h4 id={`server-${server.id}`} class="font-semibold">{server.name}</h4><span class="rounded-full bg-white/10 px-2 py-0.5 text-[10px]">{mcpStatusLabel(server)}</span></div>
              <p class="mt-1 truncate font-mono text-[11px] text-muted-foreground">{server.transport === "stdio" ? `${server.command} ${server.args.join(" ")}` : server.url}</p>
              <p class="mt-1 text-[11px] text-muted-foreground">{server.transport === "stdio" ? "Local stdio" : server.authMode === "bearer" ? "Static bearer" : server.authMode === "oauth" ? "OAuth" : server.authMode === "none" ? "No authentication" : "Automatic authentication"} · {server.tools.length} tools</p>
              {#if server.error}<p class="mt-2 text-xs text-red-300" role="alert"><AlertCircle class="mr-1 inline size-3" />{server.error}</p>{/if}
            </div>
            <div class="flex flex-wrap gap-1">
              {#if server.authStatus === "required" || server.authStatus === "error"}<Button size="sm" onclick={() => beginSignIn(server.id)}><Globe2 class="mr-1 size-3" />Sign in</Button>{/if}
              {#if server.authStatus === "pending"}<Button size="sm" onclick={() => beginSignIn(server.id)} disabled={busy}><LoaderCircle class="mr-1 size-3 animate-spin" />Start again</Button>{/if}
              {#if server.connectionStatus === "error"}<Button size="sm" variant="outline" onclick={() => runAction(() => mcpServerService.reconnect(server.id))}><RotateCw class="mr-1 size-3" />Retry</Button>{/if}
              {#if server.connectionStatus === "connected"}<Button size="sm" variant="outline" onclick={() => runAction(() => mcpServerService.disconnect(server.id))}><LogOut class="mr-1 size-3" />Disconnect</Button>{/if}
              {#if server.connectionStatus === "disabled"}<Button size="sm" variant="outline" onclick={() => runAction(() => mcpServerService.reconnect(server.id))}>Enable</Button>{/if}
              <Button size="sm" variant="ghost" onclick={() => startEdit(server)} aria-label={`Edit ${server.name}`}><Pencil class="size-3" /></Button>
              <Button size="sm" variant="ghost" onclick={() => removeServer(server)} aria-label={`Remove ${server.name}`}><Trash2 class="size-3" /></Button>
            </div>
          </div>
          {#if editingId === server.id}
            <div class="mt-3 grid gap-2 rounded-lg border border-white/10 p-3 xl:grid-cols-2">
              <label class="grid gap-1 text-xs" for={`edit-name-${server.id}`}><span>Name</span><Input id={`edit-name-${server.id}`} class={fieldClass} bind:value={editName} /></label>
              <label class="grid gap-1 text-xs" for={`edit-location-${server.id}`}><span>{server.transport === "stdio" ? "Command and arguments" : "URL"}</span><Input id={`edit-location-${server.id}`} class={fieldClass} bind:value={editLocation} /></label>
              {#if server.transport === "streamable_http"}<label class="grid gap-1 text-xs" for={`edit-auth-${server.id}`}><span>Authentication</span><select id={`edit-auth-${server.id}`} bind:value={editAuthMode} class="h-9 rounded border border-white/15 bg-background px-2"><option value="auto">Automatic</option><option value="none">None</option><option value="oauth">OAuth</option><option value="bearer">Static bearer</option></select></label>{/if}
              {#if editAuthMode === "bearer"}<label class="grid gap-1 text-xs" for={`edit-token-${server.id}`}><span>Replace bearer token</span><Input id={`edit-token-${server.id}`} class={fieldClass} type="password" bind:value={editBearerToken} /></label>{/if}
              <div class="flex gap-2 xl:col-span-2"><Button size="sm" onclick={() => saveEdit(server)}>Save</Button><Button size="sm" variant="ghost" onclick={() => editingId = null}>Cancel</Button></div>
            </div>
          {/if}
          {#if server.connectionStatus === "connected"}
            <div class="mt-3 space-y-1">{#each server.tools as tool}<label class="flex items-start justify-between gap-3 rounded border border-white/10 px-3 py-2 text-xs"><span><b>{tool.remoteName}</b><span class="block text-[10px] text-muted-foreground">{tool.description || "No description"}</span></span><span class="flex items-center gap-2"><span>New chats</span><input aria-label={`Use ${tool.remoteName} in new chats`} type="checkbox" checked={tool.enabledForNewSessions} onchange={(event) => runAction(() => mcpServerService.setToolEnabled(server.id, tool.remoteName, event.currentTarget.checked))} /></span></label>{/each}</div>
            <Button class="mt-3" size="sm" onclick={() => useInNewChat(server)} disabled={!$isFirstMessage || $selectedMcpServerIds.includes(server.id)}>{$selectedMcpServerIds.includes(server.id) ? "Selected for new chat" : "Use in new chat"}</Button>
          {/if}
        </article>
      {/each}
    {/if}
  </div>
</section>
