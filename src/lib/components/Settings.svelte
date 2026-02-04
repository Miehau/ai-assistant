<script lang="ts">
  import { open } from "@tauri-apps/api/dialog";
  import { X } from "lucide-svelte";
  import { backend } from "$lib/backend";
  import type { ToolMetadata } from "$lib/types/tools";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";
  import Integrations from "$lib/components/Integrations.svelte";

  let { showClose = false, onClose }: { showClose?: boolean; onClose?: () => void } = $props();

  const VAULT_ROOT_KEY = "plugins.files.vault_root";
  let vaultRoot = $state("");
  let savedRoot = $state<string | null>(null);
  let isLoading = $state(true);
  let isSaving = $state(false);
  let statusMessage = $state("");
  let statusTone = $state<"idle" | "success" | "error">("idle");

  let tools = $state<ToolMetadata[]>([]);
  let toolsLoading = $state(true);
  let toolsError = $state("");
  let toolQuery = $state("");
  let selectedToolName = $state<string | null>(null);
  let activeTab = $state<"tools" | "vault" | "integrations">("tools");

  $effect(() => {
    if (activeTab === "tools") {
      loadTools();
    }
    if (activeTab === "vault") {
      loadVaultRoot();
    }
  });

  async function loadVaultRoot() {
    isLoading = true;
    try {
      savedRoot = await backend.getPreference(VAULT_ROOT_KEY);
      vaultRoot = savedRoot ?? "";
      statusMessage = "";
      statusTone = "idle";
    } catch (error) {
      statusMessage = "Failed to load vault root.";
      statusTone = "error";
    } finally {
      isLoading = false;
    }
  }

  async function browseVaultRoot() {
    const selected = await open({
      directory: true,
      multiple: false,
    });
    if (typeof selected === "string") {
      vaultRoot = selected;
      statusMessage = "";
      statusTone = "idle";
    }
  }

  async function saveVaultRoot() {
    const trimmed = vaultRoot.trim();
    if (!trimmed) {
      statusMessage = "Vault root is required.";
      statusTone = "error";
      return;
    }

    isSaving = true;
    try {
      await backend.setPreference(VAULT_ROOT_KEY, trimmed);
      savedRoot = trimmed;
      statusMessage = "Vault root saved.";
      statusTone = "success";
    } catch (error) {
      statusMessage = "Failed to save vault root.";
      statusTone = "error";
    } finally {
      isSaving = false;
    }
  }

  async function loadTools() {
    toolsLoading = true;
    toolsError = "";
    try {
      tools = await backend.listTools();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toolsError = message ? `Failed to load tools: ${message}` : "Failed to load tools.";
    } finally {
      toolsLoading = false;
    }
  }

  let filteredTools = $derived(() => {
    const query = toolQuery.trim().toLowerCase();
    if (!query) return tools;
    return tools.filter((tool) => {
      const nameMatch = tool.name.toLowerCase().includes(query);
      const descMatch = tool.description?.toLowerCase().includes(query);
      return nameMatch || descMatch;
    });
  });

  let selectedTool = $derived(
    () => filteredTools.find((tool) => tool.name === selectedToolName) ?? null
  );

  $effect(() => {
    if (!filteredTools.length) {
      selectedToolName = null;
      return;
    }
    if (!selectedToolName || !filteredTools.some((tool) => tool.name === selectedToolName)) {
      selectedToolName = filteredTools[0].name;
    }
  });

  function formatSchema(schema: ToolMetadata["args_schema"] | ToolMetadata["result_schema"]): string {
    try {
      return JSON.stringify(schema, null, 2);
    } catch {
      return "";
    }
  }

  function selectTool(name: string) {
    selectedToolName = name;
  }

  function closePanel() {
    onClose?.();
  }
</script>

<div class="h-full flex flex-col gap-4 p-6">
  <div class="flex items-start justify-between gap-4">
    <div>
      <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Settings</p>
      <h2 class="text-xl font-semibold">Configuration</h2>
      <p class="text-xs text-muted-foreground/70 mt-1">
        Manage vault preferences, integrations, and tool behavior.
      </p>
    </div>
    {#if showClose}
      <Button variant="ghost" size="icon" class="rounded-lg" onclick={closePanel}>
        <X class="h-4 w-4" />
      </Button>
    {/if}
  </div>

  <div class="inline-flex items-center gap-1 rounded-xl border border-white/10 bg-white/5 p-1 text-[11px]">
    <button
      class={`px-3 py-1 rounded-lg transition-all ${
        activeTab === "tools"
          ? "bg-emerald-500/15 text-emerald-200 border border-emerald-400/40"
          : "text-muted-foreground/70 hover:text-foreground"
      }`}
      onclick={() => (activeTab = "tools")}
    >
      Tools
    </button>
    <button
      class={`px-3 py-1 rounded-lg transition-all ${
        activeTab === "vault"
          ? "bg-emerald-500/15 text-emerald-200 border border-emerald-400/40"
          : "text-muted-foreground/70 hover:text-foreground"
      }`}
      onclick={() => (activeTab = "vault")}
    >
      Vault
    </button>
    <button
      class={`px-3 py-1 rounded-lg transition-all ${
        activeTab === "integrations"
          ? "bg-emerald-500/15 text-emerald-200 border border-emerald-400/40"
          : "text-muted-foreground/70 hover:text-foreground"
      }`}
      onclick={() => (activeTab = "integrations")}
    >
      Integrations
    </button>
  </div>

  <div class="flex-1 min-h-0 rounded-2xl border border-white/10 bg-white/5 p-3">
    {#if activeTab === "tools"}
      <div class="grid h-full min-h-0 grid-cols-[minmax(220px,0.38fr)_minmax(0,0.62fr)] gap-3">
        <div class="flex min-h-0 flex-col">
          <div class="flex items-center gap-2">
            <Input
              placeholder="Search tools..."
              bind:value={toolQuery}
              class="h-8 text-xs bg-white/5 border-white/10"
            />
            <span class="glass-badge-sm text-[10px] text-muted-foreground/70">
              {filteredTools.length}
            </span>
          </div>

          <div class="mt-2 flex-1 min-h-0 overflow-y-auto space-y-1 pr-1">
            {#if toolsLoading}
              <p class="text-xs text-muted-foreground">Loading tools...</p>
            {:else if toolsError}
              <div class="flex items-center justify-between gap-2">
                <p class="text-xs text-red-400">{toolsError}</p>
                <Button
                  variant="outline"
                  size="sm"
                  class="h-7 text-[10px] border-white/10"
                  onclick={loadTools}
                  disabled={toolsLoading}
                >
                  Retry
                </Button>
              </div>
            {:else if filteredTools.length === 0}
              <p class="text-xs text-muted-foreground">No tools found.</p>
            {:else}
              {#each filteredTools as tool (tool.name)}
                <button
                  class={`w-full text-left rounded-lg border px-2 py-1.5 transition-all ${
                    tool.name === selectedToolName
                      ? "border-emerald-400/40 bg-emerald-500/10"
                      : "border-white/10 bg-white/5 hover:bg-white/10"
                  }`}
                  onclick={() => selectTool(tool.name)}
                >
                  <div class="flex items-center justify-between gap-2">
                    <span class="text-xs font-medium truncate">{tool.name}</span>
                    {#if tool.requires_approval}
                      <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-0.5 bg-amber-500/15 text-amber-300">
                        Approval
                      </span>
                    {/if}
                  </div>
                  <p class="text-[11px] text-muted-foreground/70 truncate">
                    {tool.description || "No description"}
                  </p>
                </button>
              {/each}
            {/if}
          </div>
        </div>

        <div class="min-h-0 overflow-y-auto border-l border-white/10 pl-3">
          {#if toolsLoading}
            <p class="text-xs text-muted-foreground">Loading tools...</p>
          {:else if toolsError}
            <p class="text-xs text-red-400">{toolsError}</p>
          {:else if !selectedTool}
            <p class="text-xs text-muted-foreground">Select a tool to view settings.</p>
          {:else}
            <div class="space-y-3">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <h3 class="text-sm font-semibold">{selectedTool.name}</h3>
                  <p class="text-xs text-muted-foreground/70 mt-1">
                    {selectedTool.description || "No description available."}
                  </p>
                </div>
                <span
                  class={`text-[10px] uppercase tracking-wide rounded-full px-2 py-1 ${
                    selectedTool.requires_approval
                      ? "bg-amber-500/15 text-amber-300"
                      : "bg-emerald-500/15 text-emerald-300"
                  }`}
                >
                  {selectedTool.requires_approval ? "Approval" : "Auto"}
                </span>
              </div>

              <div class="grid gap-2 text-xs">
                <div class="grid grid-cols-[120px_1fr] items-center gap-2">
                  <span class="text-muted-foreground/70">Approval</span>
                  <span>
                    {selectedTool.requires_approval ? "Ask each time" : "Auto-approved"}
                  </span>
                </div>
                <div class="grid grid-cols-[120px_1fr] items-center gap-2">
                  <span class="text-muted-foreground/70">Schemas</span>
                  <span>Args + Result</span>
                </div>
              </div>

              <details class="rounded-lg border border-white/10 bg-white/5 p-2">
                <summary class="cursor-pointer text-xs font-medium text-muted-foreground/80">
                  Args schema
                </summary>
                <pre class="mt-2 max-h-64 overflow-auto rounded-md bg-muted/40 p-2 text-[11px] font-mono text-foreground">
{formatSchema(selectedTool.args_schema)}
                </pre>
              </details>

              <details class="rounded-lg border border-white/10 bg-white/5 p-2">
                <summary class="cursor-pointer text-xs font-medium text-muted-foreground/80">
                  Result schema
                </summary>
                <pre class="mt-2 max-h-64 overflow-auto rounded-md bg-muted/40 p-2 text-[11px] font-mono text-foreground">
{formatSchema(selectedTool.result_schema)}
                </pre>
              </details>
            </div>
          {/if}
        </div>
      </div>
    {:else if activeTab === "vault"}
      <div class="grid gap-4 text-xs">
        <div class="grid grid-cols-[160px_1fr] gap-4 items-start">
          <div>
            <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Vault root</p>
            <p class="text-[11px] text-muted-foreground/70 mt-1">
              Base directory for file tooling.
            </p>
          </div>
          <div class="space-y-2">
            <div class="flex flex-wrap gap-2 items-center">
              <Input
                placeholder={isLoading ? "Loading..." : "/path/to/your/vault"}
                bind:value={vaultRoot}
                class="h-8 text-xs flex-1 min-w-[240px] bg-white/5 border-white/10"
                readonly={isLoading}
              />
              <Button variant="outline" size="sm" onclick={browseVaultRoot} disabled={isLoading}>
                Browse
              </Button>
              <Button size="sm" onclick={saveVaultRoot} disabled={isLoading || isSaving}>
                {isSaving ? "Saving..." : "Save"}
              </Button>
            </div>
            <p class="text-[11px] text-muted-foreground">
              Current value:
              <span class="font-mono">{savedRoot || "Not set"}</span>
            </p>
            {#if statusMessage}
              <p
                class={`text-[11px] ${
                  statusTone === "success"
                    ? "text-emerald-400"
                    : statusTone === "error"
                      ? "text-red-400"
                      : "text-muted-foreground"
                }`}
              >
                {statusMessage}
              </p>
            {/if}
          </div>
        </div>
      </div>
    {:else if activeTab === "integrations"}
      <div class="h-full min-h-0 overflow-y-auto pr-1">
        <div class="mb-3">
          <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Integrations</p>
          <h3 class="text-sm font-semibold">Connections and MCP</h3>
          <p class="text-[11px] text-muted-foreground/70 mt-1">
            Manage OAuth connections, tokens, and MCP servers.
          </p>
        </div>
        <Integrations embedded />
      </div>
    {/if}
  </div>
</div>
