<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/api/dialog";
  import { backend } from "$lib/backend";
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

  onMount(() => {
    loadVaultRoot();
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
          <Button variant="outline" size="sm" on:click={browseVaultRoot} disabled={isLoading}>
            Browse
          </Button>
          <Button size="sm" on:click={saveVaultRoot} disabled={isLoading || isSaving}>
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
</div>
