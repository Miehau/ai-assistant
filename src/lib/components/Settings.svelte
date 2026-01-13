<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/api/dialog";
  import { backend } from "$lib/backend";
  import type { ToolMetadata } from "$lib/types/tools";
  import * as Card from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";

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

  onMount(() => {
    loadVaultRoot();
    loadTools();
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
      toolsError = "Failed to load tools.";
    } finally {
      toolsLoading = false;
    }
  }

  function formatSchema(schema: ToolMetadata["args_schema"] | ToolMetadata["result_schema"]): string {
    try {
      return JSON.stringify(schema, null, 2);
    } catch {
      return "";
    }
  }
</script>

<div class="container max-w-3xl mx-auto py-8">
  <div class="mb-6">
    <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Settings</p>
    <h2 class="text-2xl font-semibold">Vault configuration</h2>
    <p class="text-sm text-muted-foreground/70 mt-1">
      File and search tools operate inside your Obsidian vault root.
    </p>
  </div>

  <Card.Root class="surface-card border-0 overflow-hidden">
    <Card.Content class="p-6 space-y-4">
      <div class="space-y-2">
        <label class="text-xs font-medium text-muted-foreground">Vault root path</label>
        <div class="flex flex-wrap gap-2 items-center">
          <Input
            placeholder={isLoading ? "Loading..." : "/path/to/your/vault"}
            bind:value={vaultRoot}
            class="flex-1 min-w-[240px]"
            readonly={isLoading}
          />
          <Button variant="outline" size="sm" onclick={browseVaultRoot} disabled={isLoading}>
            Browse
          </Button>
          <Button size="sm" onclick={saveVaultRoot} disabled={isLoading || isSaving}>
            {isSaving ? "Saving..." : "Save"}
          </Button>
        </div>
        <p class="text-xs text-muted-foreground">
          Current value:
          <span class="font-mono">{savedRoot || "Not set"}</span>
        </p>
        {#if statusMessage}
          <p
            class={`text-xs ${
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
    </Card.Content>
  </Card.Root>

  <div class="mt-8 mb-4">
    <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Tooling</p>
    <h3 class="text-xl font-semibold">Tool catalog</h3>
    <p class="text-sm text-muted-foreground/70 mt-1">
      Available tools and their approval requirements.
    </p>
  </div>

  <Card.Root class="surface-card border-0 overflow-hidden">
    <Card.Content class="p-6 space-y-4">
      {#if toolsLoading}
        <p class="text-sm text-muted-foreground">Loading tools...</p>
      {:else if toolsError}
        <p class="text-sm text-red-400">{toolsError}</p>
      {:else if tools.length === 0}
        <p class="text-sm text-muted-foreground">No tools registered.</p>
      {:else}
        <div class="space-y-4">
          {#each tools as tool (tool.name)}
            <details class="rounded-xl border border-border/40 bg-background/40 px-4 py-3">
              <summary class="cursor-pointer list-none">
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <p class="text-sm font-semibold text-foreground">{tool.name}</p>
                    <p class="text-xs text-muted-foreground">{tool.description}</p>
                  </div>
                  <span
                    class={`text-[10px] uppercase tracking-wide rounded-full px-2 py-1 ${
                      tool.requires_approval
                        ? "bg-amber-500/15 text-amber-300"
                        : "bg-emerald-500/15 text-emerald-300"
                    }`}
                  >
                    {tool.requires_approval ? "Approval" : "No approval"}
                  </span>
                </div>
              </summary>

              <div class="mt-3 grid gap-3 md:grid-cols-2">
                <div>
                  <p class="text-xs font-medium text-muted-foreground mb-1">Args schema</p>
                  <pre class="max-h-64 overflow-auto rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">
{formatSchema(tool.args_schema)}
                  </pre>
                </div>
                <div>
                  <p class="text-xs font-medium text-muted-foreground mb-1">Result schema</p>
                  <pre class="max-h-64 overflow-auto rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">
{formatSchema(tool.result_schema)}
                  </pre>
                </div>
              </div>
            </details>
          {/each}
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
</div>
