<script lang="ts">
  import { onMount } from "svelte";
  import { backend } from "$lib/backend";
  import { open as openExternal } from "@tauri-apps/api/shell";
  import * as Card from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";
  import { Switch } from "$lib/components/ui/switch";
  import * as Select from "$lib/components/ui/select";
  import { Plus, Edit2, Check, X, Eye, EyeOff, Trash2 } from "lucide-svelte";
  import type { IntegrationMetadata } from "$lib/types/integrations";
  import type { McpServer } from "$lib/types/mcpServer";
  import type { IntegrationConnection } from "$lib/types/integrationConnection";
  import { mcpServerService } from "$lib/services/mcpServerService.svelte";
  import { integrationConnectionService } from "$lib/services/integrationConnectionService.svelte";

  let { embedded = false }: { embedded?: boolean } = $props();

  let integrations = $state<IntegrationMetadata[]>([]);
  let integrationsLoading = $state(true);
  let integrationsError = $state("");
  let connections = $derived(integrationConnectionService.connections);
  let gmailConnection = $derived(connections.find((item) => item.integration_id === "gmail"));

  let oauthSessionId = $state<string | null>(null);
  let oauthStatus = $state<"idle" | "pending" | "completed" | "error" | "cancelled">("idle");
  let oauthError = $state("");
  let oauthLoading = $state(false);

  let useCustomOAuth = $state(false);
  let customClientId = $state("");
  let customClientSecret = $state("");
  let showCustomSecret = $state(false);
  let oauthPrefsLoading = $state(false);
  let oauthPrefsMessage = $state("");
  let oauthPrefsTone = $state<"idle" | "success" | "error">("idle");

  let isAddingConnection = $state(false);
  let newConnectionIntegrationId = $state("google_calendar");
  let newConnectionAccountLabel = $state("");
  let newConnectionAccessToken = $state("");
  let newConnectionRefreshToken = $state("");
  let newConnectionScopes = $state("");
  let newConnectionExpiresAt = $state("");
  let showNewConnectionAccessToken = $state(false);
  let showNewConnectionRefreshToken = $state(false);

  let editingConnectionId = $state<string | null>(null);
  let editConnectionAccountLabel = $state("");
  let editConnectionAccessToken = $state("");
  let editConnectionRefreshToken = $state("");
  let editConnectionScopes = $state("");
  let editConnectionExpiresAt = $state("");
  let showEditConnectionAccessToken = $state(false);
  let showEditConnectionRefreshToken = $state(false);

  let newName = $state("");
  let newUrl = $state("");
  let newAuthType = $state("api_key");
  let newApiKey = $state("");
  let showNewApiKey = $state(false);
  let isAdding = $state(false);
  let isLoading = $state(false);

  let editingId = $state<string | null>(null);
  let editName = $state("");
  let editUrl = $state("");
  let editAuthType = $state("api_key");
  let editApiKey = $state("");
  let showEditApiKey = $state(false);

  const authOptions = [
    { value: "none", label: "No auth" },
    { value: "api_key", label: "API key" }
  ];
  const connectionIntegrations = $derived(
    integrations.filter((item) => item.id !== "mcp" && item.id !== "gmail")
  );

  onMount(async () => {
    await loadIntegrations();
    await integrationConnectionService.loadConnections();
    await mcpServerService.loadServers();
    await loadOAuthPrefs();
  });

  async function loadIntegrations() {
    integrationsLoading = true;
    integrationsError = "";
    try {
      integrations = await backend.listIntegrations();
      ensureDefaultIntegrationSelection();
    } catch (error) {
      integrationsError = "Failed to load integrations.";
    } finally {
      integrationsLoading = false;
    }
  }

  async function loadOAuthPrefs() {
    oauthPrefsLoading = true;
    oauthPrefsMessage = "";
    oauthPrefsTone = "idle";
    try {
      const useCustom = await backend.getPreference("oauth.google.use_custom");
      const clientId = await backend.getPreference("oauth.google.client_id");
      const clientSecret = await backend.getPreference("oauth.google.client_secret");
      useCustomOAuth = useCustom === "true";
      customClientId = clientId ?? "";
      customClientSecret = clientSecret ?? "";
    } catch (error) {
      oauthPrefsMessage = "Failed to load OAuth settings.";
      oauthPrefsTone = "error";
    } finally {
      oauthPrefsLoading = false;
    }
  }

  function ensureDefaultIntegrationSelection() {
    if (!newConnectionIntegrationId) {
      const first = integrations.find((item) => item.id !== "mcp" && item.id !== "gmail");
      if (first) {
        newConnectionIntegrationId = first.id;
      }
    }
  }

  function authLabel(value: string) {
    return authOptions.find((option) => option.value === value)?.label ?? value;
  }

  function integrationLabel(id: string) {
    if (!id) return "Select integration";
    return integrations.find((item) => item.id === id)?.name ?? id;
  }

  function connectionAuthType(id: string) {
    return integrations.find((item) => item.id === id)?.auth_type ?? "oauth2";
  }

  function statusBadge(status: string) {
    switch (status) {
      case "connected":
        return "bg-emerald-500/15 text-emerald-300";
      case "error":
        return "bg-red-500/15 text-red-300";
      case "disconnected":
        return "bg-amber-500/15 text-amber-300";
      default:
        return "bg-white/5 text-muted-foreground";
    }
  }

  function connectionCount(id: string) {
    return connections.filter((item) => item.integration_id === id).length;
  }

  async function connectGmail() {
    oauthError = "";
    oauthLoading = true;
    try {
      const response = await backend.startGoogleOAuth("gmail");
      oauthSessionId = response.session_id;
      oauthStatus = "pending";
      await openExternal(response.auth_url);
      pollOAuth(response.session_id);
    } catch (error) {
      oauthError = error instanceof Error ? error.message : String(error);
      oauthStatus = "error";
    } finally {
      oauthLoading = false;
    }
  }

  async function pollOAuth(sessionId: string) {
    try {
      const status = await backend.getOauthSession(sessionId);
      const nextStatus =
        status.status === "pending" ||
        status.status === "completed" ||
        status.status === "error" ||
        status.status === "cancelled"
          ? status.status
          : "error";
      oauthStatus = nextStatus;

      if (nextStatus === "completed") {
        await integrationConnectionService.loadConnections();
        oauthSessionId = null;
        return;
      }
      if (nextStatus === "error" || nextStatus === "cancelled") {
        oauthError = status.error || (nextStatus === "cancelled" ? "OAuth cancelled." : "");
        oauthSessionId = null;
        return;
      }

      setTimeout(() => pollOAuth(sessionId), 1000);
    } catch (error) {
      oauthError = error instanceof Error ? error.message : String(error);
      oauthStatus = "error";
      oauthSessionId = null;
    }
  }

  async function cancelOAuth() {
    if (!oauthSessionId) return;
    try {
      await backend.cancelOauthSession(oauthSessionId);
      oauthStatus = "cancelled";
      oauthSessionId = null;
    } catch (error) {
      oauthError = error instanceof Error ? error.message : String(error);
      oauthStatus = "error";
    }
  }

  async function saveOAuthPrefs() {
    oauthPrefsLoading = true;
    oauthPrefsMessage = "";
    oauthPrefsTone = "idle";
    try {
      await backend.setPreference("oauth.google.use_custom", useCustomOAuth ? "true" : "false");
      await backend.setPreference("oauth.google.client_id", customClientId.trim());
      await backend.setPreference("oauth.google.client_secret", customClientSecret.trim());
      oauthPrefsMessage = "OAuth settings saved.";
      oauthPrefsTone = "success";
    } catch (error) {
      oauthPrefsMessage = "Failed to save OAuth settings.";
      oauthPrefsTone = "error";
    } finally {
      oauthPrefsLoading = false;
    }
  }

  async function addConnection() {
    if (!newConnectionIntegrationId || !newConnectionAccessToken.trim()) return;

    isLoading = true;
    try {
      const authType = connectionAuthType(newConnectionIntegrationId);
      const expiresAt = newConnectionExpiresAt.trim()
        ? Number(newConnectionExpiresAt.trim())
        : undefined;
      const connection = await integrationConnectionService.createConnection({
        integration_id: newConnectionIntegrationId,
        account_label: newConnectionAccountLabel.trim() || undefined,
        auth_type: authType,
        access_token: newConnectionAccessToken.trim() || undefined,
        refresh_token: newConnectionRefreshToken.trim() || undefined,
        scopes: newConnectionScopes.trim() || undefined,
        expires_at: Number.isFinite(expiresAt) ? expiresAt : undefined
      });
      if (connection) {
        newConnectionAccountLabel = "";
        newConnectionAccessToken = "";
        newConnectionRefreshToken = "";
        newConnectionScopes = "";
        newConnectionExpiresAt = "";
        if (newConnectionIntegrationId === "gmail") {
          newConnectionIntegrationId = "google_calendar";
        }
        isAddingConnection = false;
      }
    } finally {
      isLoading = false;
    }
  }

  function startConnectionEdit(connection: IntegrationConnection) {
    editingConnectionId = connection.id;
    editConnectionAccountLabel = connection.account_label || "";
    editConnectionAccessToken = connection.access_token || "";
    editConnectionRefreshToken = connection.refresh_token || "";
    editConnectionScopes = connection.scopes || "";
    editConnectionExpiresAt = connection.expires_at ? String(connection.expires_at) : "";
    showEditConnectionAccessToken = false;
    showEditConnectionRefreshToken = false;
  }

  function cancelConnectionEdit() {
    editingConnectionId = null;
    editConnectionAccountLabel = "";
    editConnectionAccessToken = "";
    editConnectionRefreshToken = "";
    editConnectionScopes = "";
    editConnectionExpiresAt = "";
    showEditConnectionAccessToken = false;
    showEditConnectionRefreshToken = false;
  }

  async function saveConnectionEdit(connection: IntegrationConnection) {
    if (!editingConnectionId) return;

    isLoading = true;
    try {
      const expiresAt = editConnectionExpiresAt.trim()
        ? Number(editConnectionExpiresAt.trim())
        : undefined;
      await integrationConnectionService.updateConnection({
        id: editingConnectionId,
        account_label: editConnectionAccountLabel.trim() || undefined,
        access_token: editConnectionAccessToken.trim() || undefined,
        refresh_token: editConnectionRefreshToken.trim() || undefined,
        scopes: editConnectionScopes.trim() || undefined,
        expires_at: Number.isFinite(expiresAt) ? expiresAt : undefined,
        auth_type: connection.auth_type,
        status: connection.status
      });
      cancelConnectionEdit();
    } finally {
      isLoading = false;
    }
  }

  async function deleteConnection(id: string) {
    if (!confirm("Are you sure you want to delete this integration connection?")) return;

    isLoading = true;
    try {
      await integrationConnectionService.deleteConnection(id);
    } finally {
      isLoading = false;
    }
  }

  async function disconnectGmail() {
    if (!gmailConnection) return;
    if (!confirm("Disconnect Gmail and delete stored tokens?")) return;

    isLoading = true;
    try {
      await integrationConnectionService.deleteConnection(gmailConnection.id);
      await integrationConnectionService.loadConnections();
    } finally {
      isLoading = false;
    }
  }

  async function testConnection(id: string) {
    isLoading = true;
    try {
      await integrationConnectionService.testConnection(id);
      await integrationConnectionService.loadConnections();
    } finally {
      isLoading = false;
    }
  }

  async function addServer() {
    if (!newName.trim() || !newUrl.trim()) return;

    isLoading = true;
    try {
      const apiKey = newAuthType === "api_key" ? newApiKey.trim() || undefined : undefined;
      const server = await mcpServerService.createServer({
        name: newName.trim(),
        url: newUrl.trim(),
        auth_type: newAuthType,
        api_key: apiKey
      });
      if (server) {
        newName = "";
        newUrl = "";
        newAuthType = "api_key";
        newApiKey = "";
        isAdding = false;
      }
    } finally {
      isLoading = false;
    }
  }

  function startEdit(server: McpServer) {
    editingId = server.id;
    editName = server.name;
    editUrl = server.url;
    editAuthType = server.auth_type || "api_key";
    editApiKey = server.api_key || "";
    showEditApiKey = false;
  }

  function cancelEdit() {
    editingId = null;
    editName = "";
    editUrl = "";
    editAuthType = "api_key";
    editApiKey = "";
    showEditApiKey = false;
  }

  async function saveEdit() {
    if (!editingId || !editName.trim() || !editUrl.trim()) return;

    isLoading = true;
    try {
      const apiKey = editAuthType === "api_key" ? editApiKey.trim() : "";
      await mcpServerService.updateServer({
        id: editingId,
        name: editName.trim(),
        url: editUrl.trim(),
        auth_type: editAuthType,
        api_key: apiKey
      });
      cancelEdit();
    } finally {
      isLoading = false;
    }
  }

  async function deleteServer(id: string) {
    if (!confirm("Are you sure you want to delete this MCP server?")) return;

    isLoading = true;
    try {
      await mcpServerService.deleteServer(id);
    } finally {
      isLoading = false;
    }
  }

  async function testServer(id: string) {
    isLoading = true;
    try {
      await mcpServerService.testServer(id);
    } finally {
      isLoading = false;
    }
  }
</script>

<div class={embedded ? "space-y-5" : "mt-8"}>
  <div class={embedded ? "mb-2" : "mb-6"}>
    {#if embedded}
      <h3 class="text-sm font-semibold">Integration catalog</h3>
      <p class="text-[11px] text-muted-foreground/70 mt-1">
        Available plugins and their capabilities.
      </p>
    {:else}
      <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Integrations</p>
      <h3 class="text-xl font-semibold">Integration catalog</h3>
      <p class="text-sm text-muted-foreground/70 mt-1">
        Available plugins and their capabilities. Configure MCP servers below.
      </p>
    {/if}
  </div>

  <Card.Root
    class={embedded ? "border-white/10 bg-white/5 shadow-none rounded-2xl" : "surface-card border-0 overflow-hidden"}
  >
    <Card.Content class={embedded ? "p-4 pt-4 space-y-4" : "p-6 space-y-4"}>
      {#if integrationsLoading}
        <p class="text-sm text-muted-foreground">Loading integrations...</p>
      {:else if integrationsError}
        <p class="text-sm text-red-400">{integrationsError}</p>
      {:else if integrations.length === 0}
        <p class="text-sm text-muted-foreground">No integrations registered.</p>
      {:else}
        <div class="space-y-4">
          {#each integrations as integration (integration.id)}
            <div
              class={`rounded-xl border px-4 py-3 ${
                embedded ? "border-white/10 bg-white/5" : "border-border/40 bg-background/40"
              }`}
            >
              <div class="flex flex-wrap items-start justify-between gap-3">
                <div class="min-w-[220px]">
                  <p class="text-sm font-semibold text-foreground">{integration.name}</p>
                  <p class="text-xs text-muted-foreground">{integration.description}</p>
                  <div class="mt-2 flex flex-wrap gap-2">
                    <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-1 bg-white/5 text-muted-foreground">
                      {integration.category}
                    </span>
                    <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-1 bg-white/5 text-muted-foreground">
                      {integration.auth_type}
                    </span>
                    {#each integration.capabilities as capability}
                      <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-1 bg-emerald-500/10 text-emerald-300">
                        {capability}
                      </span>
                    {/each}
                  </div>
                </div>
                {#if connectionCount(integration.id) > 0}
                  <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-1 bg-emerald-500/15 text-emerald-300">
                    Connected ({connectionCount(integration.id)})
                  </span>
                {:else}
                  <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-1 bg-amber-500/15 text-amber-300">
                    Not configured
                  </span>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </Card.Content>
  </Card.Root>

  <div class={embedded ? "mt-5 mb-2" : "mt-8 mb-4"}>
    {#if embedded}
      <h3 class="text-sm font-semibold">Integration connections</h3>
      <p class="text-[11px] text-muted-foreground/70 mt-1">
        OAuth and token connections for Gmail, Google Calendar, and Todoist.
      </p>
    {:else}
      <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Connections</p>
      <h3 class="text-xl font-semibold">Integration connections</h3>
      <p class="text-sm text-muted-foreground/70 mt-1">
        Store credentials for Gmail, Google Calendar, and Todoist connections.
      </p>
    {/if}
  </div>

  <Card.Root
    class={embedded ? "border-white/10 bg-white/5 shadow-none rounded-2xl" : "surface-card border-0 overflow-hidden"}
  >
    <Card.Content class={embedded ? "p-4 pt-4" : "p-6"}>
      <div class="space-y-6">
        <div
          class={`rounded-xl border px-4 py-4 ${
            embedded ? "border-white/10 bg-white/5" : "border-border/40 bg-background/40"
          }`}
        >
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p class="text-sm font-semibold text-foreground">Gmail OAuth</p>
              <p class="text-xs text-muted-foreground">
                {#if gmailConnection}
                  Connected as {gmailConnection.account_label || "Gmail account"}.
                {:else}
                  Connect Gmail via OAuth to enable read and send tools.
                {/if}
              </p>
            </div>
            <div class="flex items-center gap-2">
              {#if oauthStatus === "pending"}
                <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-1 bg-amber-500/15 text-amber-300">
                  Waiting for approval
                </span>
                <Button size="sm" variant="ghost" onclick={cancelOAuth}>
                  Cancel
                </Button>
              {/if}
              {#if gmailConnection}
                <Button size="sm" variant="ghost" onclick={disconnectGmail} disabled={isLoading}>
                  Disconnect
                </Button>
              {/if}
              <Button
                size="sm"
                class="glass-badge hover:glass-light"
                onclick={connectGmail}
                disabled={oauthLoading || oauthStatus === "pending"}
              >
                {gmailConnection ? "Reconnect Gmail" : "Connect Gmail"}
              </Button>
            </div>
          </div>
          {#if oauthError}
            <p class="text-xs text-red-400 mt-2">{oauthError}</p>
          {/if}
        </div>

        <div
          class={`rounded-xl border px-4 py-4 ${
            embedded ? "border-white/10 bg-white/5" : "border-border/40 bg-background/40"
          }`}
        >
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p class="text-sm font-semibold text-foreground">Custom Google OAuth Client</p>
              <p class="text-xs text-muted-foreground">
                Use your own Google OAuth client ID (advanced). Applies to Gmail and Google Calendar.
              </p>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-xs text-muted-foreground">Use custom</span>
              <Switch bind:checked={useCustomOAuth} disabled={oauthPrefsLoading} />
            </div>
          </div>

          {#if useCustomOAuth}
            <div class="mt-4 space-y-3">
              <div>
                <label class="text-xs font-medium text-muted-foreground mb-1 block">Client ID</label>
                <Input
                  bind:value={customClientId}
                  placeholder="your-client-id.apps.googleusercontent.com"
                  class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                />
              </div>
              <div>
                <label class="text-xs font-medium text-muted-foreground mb-1 block">Client secret (optional)</label>
                <div class="relative">
                  <Input
                    type={showCustomSecret ? "text" : "password"}
                    bind:value={customClientSecret}
                    placeholder="Optional for desktop apps"
                    class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                  />
                  <button
                    type="button"
                    class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                    onclick={() => showCustomSecret = !showCustomSecret}
                  >
                    {#if showCustomSecret}
                      <EyeOff class="h-4 w-4" />
                    {:else}
                      <Eye class="h-4 w-4" />
                    {/if}
                  </button>
                </div>
              </div>
              <div class="flex items-center gap-2">
                <Button
                  size="sm"
                  class="glass-badge hover:glass-light"
                  onclick={saveOAuthPrefs}
                  disabled={oauthPrefsLoading}
                >
                  {oauthPrefsLoading ? "Saving..." : "Save OAuth settings"}
                </Button>
                {#if oauthPrefsMessage}
                  <p
                    class={`text-xs ${
                      oauthPrefsTone === "success"
                        ? "text-emerald-400"
                        : oauthPrefsTone === "error"
                          ? "text-red-400"
                          : "text-muted-foreground"
                    }`}
                  >
                    {oauthPrefsMessage}
                  </p>
                {/if}
              </div>
            </div>
          {/if}
        </div>

        {#if integrationConnectionService.loading}
          <p class="text-sm text-muted-foreground">Loading connections...</p>
        {:else if integrationConnectionService.error}
          <p class="text-sm text-red-400">{integrationConnectionService.error}</p>
        {:else if connections.length === 0}
          <p class="text-sm text-muted-foreground">No integration connections yet.</p>
        {:else}
          {#each connections as connection (connection.id)}
            <div class="pb-6 border-b surface-divider last:border-0 last:pb-0">
              {#if editingConnectionId === connection.id}
                <div class="space-y-3">
                  <div>
                    <label class="text-xs font-medium text-muted-foreground mb-1 block">Integration</label>
                    <p class="text-sm font-medium">{integrationLabel(connection.integration_id)}</p>
                  </div>
                  <div>
                    <label class="text-xs font-medium text-muted-foreground mb-1 block">Account label</label>
                    <Input
                      bind:value={editConnectionAccountLabel}
                      placeholder="Work Gmail"
                      class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                    />
                  </div>
                  <div>
                    <label class="text-xs font-medium text-muted-foreground mb-1 block">Access token</label>
                    <div class="relative">
                      <Input
                        type={showEditConnectionAccessToken ? "text" : "password"}
                        bind:value={editConnectionAccessToken}
                        placeholder="Access token"
                        class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                      />
                      <button
                        type="button"
                        class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                        onclick={() => showEditConnectionAccessToken = !showEditConnectionAccessToken}
                      >
                        {#if showEditConnectionAccessToken}
                          <EyeOff class="h-4 w-4" />
                        {:else}
                          <Eye class="h-4 w-4" />
                        {/if}
                      </button>
                    </div>
                  </div>
                  <div>
                    <label class="text-xs font-medium text-muted-foreground mb-1 block">Refresh token (optional)</label>
                    <div class="relative">
                      <Input
                        type={showEditConnectionRefreshToken ? "text" : "password"}
                        bind:value={editConnectionRefreshToken}
                        placeholder="Refresh token"
                        class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                      />
                      <button
                        type="button"
                        class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                        onclick={() => showEditConnectionRefreshToken = !showEditConnectionRefreshToken}
                      >
                        {#if showEditConnectionRefreshToken}
                          <EyeOff class="h-4 w-4" />
                        {:else}
                          <Eye class="h-4 w-4" />
                        {/if}
                      </button>
                    </div>
                  </div>
                  <div>
                    <label class="text-xs font-medium text-muted-foreground mb-1 block">Scopes</label>
                    <Input
                      bind:value={editConnectionScopes}
                      placeholder="https://www.googleapis.com/auth/gmail.send"
                      class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                    />
                  </div>
                  <div>
                    <label class="text-xs font-medium text-muted-foreground mb-1 block">Expires at (unix ms)</label>
                    <Input
                      bind:value={editConnectionExpiresAt}
                      placeholder="1700000000000"
                      class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                    />
                  </div>
                  <div class="flex gap-2">
                    <Button
                      size="sm"
                      class="glass-badge hover:glass-light"
                      onclick={() => saveConnectionEdit(connection)}
                      disabled={isLoading}
                    >
                      <Check class="h-4 w-4 mr-1" />
                      Save
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onclick={cancelConnectionEdit}
                    >
                      <X class="h-4 w-4 mr-1" />
                      Cancel
                    </Button>
                  </div>
                </div>
              {:else}
                <div class="flex items-start justify-between">
                  <div>
                    <h3 class="text-sm font-medium">{integrationLabel(connection.integration_id)}</h3>
                    <p class="text-xs text-muted-foreground mt-1">
                      {connection.account_label || "No account label"}
                    </p>
                    <p class="text-xs text-muted-foreground mt-0.5">
                      Auth: {connection.auth_type}
                    </p>
                    {#if connection.last_error}
                      <p class="text-xs text-red-400 mt-1">{connection.last_error}</p>
                    {/if}
                  </div>
                  <div class="flex items-center gap-2">
                    <span class={`text-[10px] uppercase tracking-wide rounded-full px-2 py-1 ${statusBadge(connection.status)}`}>
                      {connection.status}
                    </span>
                    <Button
                      size="sm"
                      variant="ghost"
                      class="h-8"
                      onclick={() => testConnection(connection.id)}
                    >
                      Test
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      class="h-8 w-8 hover:bg-white/5"
                      onclick={() => startConnectionEdit(connection)}
                    >
                      <Edit2 class="h-4 w-4" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      class="h-8 w-8 text-destructive hover:text-destructive hover:bg-white/5"
                      onclick={() => deleteConnection(connection.id)}
                    >
                      <Trash2 class="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              {/if}
            </div>
          {/each}
        {/if}

        {#if isAddingConnection && newConnectionIntegrationId !== "gmail"}
          <div class="pt-6 border-t surface-divider space-y-3">
            <h3 class="text-xs font-medium text-muted-foreground">Add integration connection</h3>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Integration</label>
              <Select.Root
                selected={{ value: newConnectionIntegrationId, label: integrationLabel(newConnectionIntegrationId) }}
                onSelectedChange={(v) => {
                  if (v) newConnectionIntegrationId = v.value;
                }}
              >
                <Select.Trigger class="w-full h-9 rounded-md border border-input bg-transparent px-3 text-sm">
                  <span>{integrationLabel(newConnectionIntegrationId)}</span>
                </Select.Trigger>
                <Select.Portal>
                  <Select.Content>
                    <Select.ScrollUpButton />
                    <Select.Viewport>
                      {#each connectionIntegrations as option}
                        <Select.Item value={option.id}>
                          {option.name}
                        </Select.Item>
                      {/each}
                    </Select.Viewport>
                    <Select.ScrollDownButton />
                  </Select.Content>
                </Select.Portal>
              </Select.Root>
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Account label</label>
              <Input
                bind:value={newConnectionAccountLabel}
                placeholder="Work Gmail"
                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
              />
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Access token</label>
              <div class="relative">
                <Input
                  type={showNewConnectionAccessToken ? "text" : "password"}
                  bind:value={newConnectionAccessToken}
                  placeholder="Access token"
                  class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                />
                <button
                  type="button"
                  class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                  onclick={() => showNewConnectionAccessToken = !showNewConnectionAccessToken}
                >
                  {#if showNewConnectionAccessToken}
                    <EyeOff class="h-4 w-4" />
                  {:else}
                    <Eye class="h-4 w-4" />
                  {/if}
                </button>
              </div>
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Refresh token (optional)</label>
              <div class="relative">
                <Input
                  type={showNewConnectionRefreshToken ? "text" : "password"}
                  bind:value={newConnectionRefreshToken}
                  placeholder="Refresh token"
                  class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                />
                <button
                  type="button"
                  class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                  onclick={() => showNewConnectionRefreshToken = !showNewConnectionRefreshToken}
                >
                  {#if showNewConnectionRefreshToken}
                    <EyeOff class="h-4 w-4" />
                  {:else}
                    <Eye class="h-4 w-4" />
                  {/if}
                </button>
              </div>
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Scopes</label>
              <Input
                bind:value={newConnectionScopes}
                placeholder="https://www.googleapis.com/auth/gmail.send"
                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
              />
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Expires at (unix ms)</label>
              <Input
                bind:value={newConnectionExpiresAt}
                placeholder="1700000000000"
                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
              />
            </div>
            <div class="flex gap-2">
              <Button
                size="sm"
                class="glass-badge hover:glass-light"
                onclick={addConnection}
                disabled={isLoading}
              >
                <Plus class="h-4 w-4 mr-1" />
                Add
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onclick={() => {
                  isAddingConnection = false;
                  newConnectionAccountLabel = "";
                  newConnectionAccessToken = "";
                  newConnectionRefreshToken = "";
                  newConnectionScopes = "";
                  newConnectionExpiresAt = "";
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        {:else if newConnectionIntegrationId !== "gmail"}
          <div class="pt-6 border-t surface-divider">
            <Button
              size="sm"
              class="glass-badge hover:glass-light"
              onclick={() => isAddingConnection = true}
            >
              <Plus class="h-4 w-4 mr-1" />
              Add integration connection
            </Button>
          </div>
        {:else}
          <div class="pt-6 border-t surface-divider">
            <p class="text-xs text-muted-foreground">
              Gmail connections are managed via OAuth above.
            </p>
          </div>
        {/if}
      </div>
    </Card.Content>
  </Card.Root>

  <div class={embedded ? "mt-5 mb-2" : "mt-8 mb-4"}>
    {#if embedded}
      <h3 class="text-sm font-semibold">MCP servers</h3>
      <p class="text-[11px] text-muted-foreground/70 mt-1">
        Add local or remote MCP servers and manage auth.
      </p>
    {:else}
      <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">MCP</p>
      <h3 class="text-xl font-semibold">MCP servers</h3>
      <p class="text-sm text-muted-foreground/70 mt-1">
        Add local or remote MCP servers, manage auth, and keep connections organized.
      </p>
    {/if}
  </div>

  <Card.Root
    class={embedded ? "border-white/10 bg-white/5 shadow-none rounded-2xl" : "surface-card border-0 overflow-hidden"}
  >
    <Card.Content class={embedded ? "p-4 pt-4" : "p-6"}>
      <div class="space-y-6">
        {#each mcpServerService.servers as server (server.id)}
          <div class="pb-6 border-b surface-divider last:border-0 last:pb-0">
            {#if editingId === server.id}
              <div class="space-y-3">
                <div>
                  <label class="text-xs font-medium text-muted-foreground mb-1 block">Name</label>
                  <Input
                    bind:value={editName}
                    placeholder="My MCP Server"
                    class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                  />
                </div>
                <div>
                  <label class="text-xs font-medium text-muted-foreground mb-1 block">URL</label>
                  <Input
                    bind:value={editUrl}
                    placeholder="http://localhost:3000"
                    class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                  />
                </div>
                <div>
                  <label class="text-xs font-medium text-muted-foreground mb-1 block">Auth type</label>
                  <Select.Root
                    selected={{ value: editAuthType, label: authLabel(editAuthType) }}
                    onSelectedChange={(v) => {
                      if (v) editAuthType = v.value;
                    }}
                  >
                    <Select.Trigger class="w-full h-9 rounded-md border border-input bg-transparent px-3 text-sm">
                      <span>{authLabel(editAuthType)}</span>
                    </Select.Trigger>
                    <Select.Portal>
                      <Select.Content>
                        <Select.ScrollUpButton />
                        <Select.Viewport>
                          {#each authOptions as option}
                            <Select.Item value={option.value}>
                              {option.label}
                            </Select.Item>
                          {/each}
                        </Select.Viewport>
                        <Select.ScrollDownButton />
                      </Select.Content>
                    </Select.Portal>
                  </Select.Root>
                </div>
                <div>
                  <label class="text-xs font-medium text-muted-foreground mb-1 block">API Key (optional)</label>
                  <div class="relative">
                    <Input
                      type={showEditApiKey ? "text" : "password"}
                      bind:value={editApiKey}
                      placeholder="API key"
                      class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                      disabled={editAuthType !== "api_key"}
                    />
                    <button
                      type="button"
                      class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                      onclick={() => showEditApiKey = !showEditApiKey}
                      disabled={editAuthType !== "api_key"}
                    >
                      {#if showEditApiKey}
                        <EyeOff class="h-4 w-4" />
                      {:else}
                        <Eye class="h-4 w-4" />
                      {/if}
                    </button>
                  </div>
                </div>
                <div class="flex gap-2">
                  <Button
                    size="sm"
                    class="glass-badge hover:glass-light"
                    onclick={saveEdit}
                    disabled={isLoading}
                  >
                    <Check class="h-4 w-4 mr-1" />
                    Save
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onclick={cancelEdit}
                  >
                    <X class="h-4 w-4 mr-1" />
                    Cancel
                  </Button>
                </div>
              </div>
            {:else}
              <div class="flex items-start justify-between">
                <div>
                  <h3 class="text-sm font-medium">{server.name}</h3>
                  <p class="text-xs text-muted-foreground mt-1 font-mono truncate max-w-md">{server.url}</p>
                  <p class="text-xs text-muted-foreground mt-0.5">Auth: {authLabel(server.auth_type)}</p>
                  {#if server.api_key}
                    <p class="text-xs text-muted-foreground mt-0.5">API key configured</p>
                  {/if}
                </div>
                <div class="flex gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    class="h-8"
                    onclick={() => testServer(server.id)}
                  >
                    Test
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    class="h-8 w-8 hover:bg-white/5"
                    onclick={() => startEdit(server)}
                  >
                    <Edit2 class="h-4 w-4" />
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    class="h-8 w-8 text-destructive hover:text-destructive hover:bg-white/5"
                    onclick={() => deleteServer(server.id)}
                  >
                    <Trash2 class="h-4 w-4" />
                  </Button>
                </div>
              </div>
            {/if}
          </div>
        {/each}

        {#if isAdding}
          <div class="pt-6 border-t surface-divider space-y-3">
            <h3 class="text-xs font-medium text-muted-foreground">Add MCP Server</h3>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Name</label>
              <Input
                bind:value={newName}
                placeholder="My MCP Server"
                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
              />
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">URL</label>
              <Input
                bind:value={newUrl}
                placeholder="http://localhost:3000"
                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
              />
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">Auth type</label>
              <Select.Root
                selected={{ value: newAuthType, label: authLabel(newAuthType) }}
                onSelectedChange={(v) => {
                  if (v) newAuthType = v.value;
                }}
              >
                <Select.Trigger class="w-full h-9 rounded-md border border-input bg-transparent px-3 text-sm">
                  <span>{authLabel(newAuthType)}</span>
                </Select.Trigger>
                <Select.Portal>
                  <Select.Content>
                    <Select.ScrollUpButton />
                    <Select.Viewport>
                      {#each authOptions as option}
                        <Select.Item value={option.value}>
                          {option.label}
                        </Select.Item>
                      {/each}
                    </Select.Viewport>
                    <Select.ScrollDownButton />
                  </Select.Content>
                </Select.Portal>
              </Select.Root>
            </div>
            <div>
              <label class="text-xs font-medium text-muted-foreground mb-1 block">API Key (optional)</label>
              <div class="relative">
                <Input
                  type={showNewApiKey ? "text" : "password"}
                  bind:value={newApiKey}
                  placeholder="API key"
                  class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                  disabled={newAuthType !== "api_key"}
                />
                <button
                  type="button"
                  class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                  onclick={() => showNewApiKey = !showNewApiKey}
                  disabled={newAuthType !== "api_key"}
                >
                  {#if showNewApiKey}
                    <EyeOff class="h-4 w-4" />
                  {:else}
                    <Eye class="h-4 w-4" />
                  {/if}
                </button>
              </div>
            </div>
            <div class="flex gap-2">
              <Button
                size="sm"
                class="glass-badge hover:glass-light"
                onclick={addServer}
                disabled={isLoading}
              >
                <Plus class="h-4 w-4 mr-1" />
                Add
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onclick={() => {
                  isAdding = false;
                  newName = "";
                  newUrl = "";
                  newAuthType = "api_key";
                  newApiKey = "";
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        {:else}
          <div class="pt-6 border-t surface-divider">
            <Button
              size="sm"
              class="glass-badge hover:glass-light"
              onclick={() => isAdding = true}
            >
              <Plus class="h-4 w-4 mr-1" />
              Add MCP server
            </Button>
          </div>
        {/if}
      </div>
    </Card.Content>
  </Card.Root>
</div>
