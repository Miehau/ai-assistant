<script lang="ts">
    import { Input } from "$lib/components/ui/input";
    import { Button } from "$lib/components/ui/button";
    import * as Card from "$lib/components/ui/card";
    import { cn } from "$lib/utils";
    import { onMount } from "svelte";
    import { Trash2, Plus, Edit2, Check, X, Eye, EyeOff } from "lucide-svelte";
    import { customBackendService } from "$lib/services/customBackendService.svelte";
    import type { CustomBackend, CreateCustomBackendInput } from "$lib/types/customBackend";

    // Form state for adding new backend
    let newName = $state("");
    let newUrl = $state("");
    let newApiKey = $state("");
    let showNewApiKey = $state(false);
    let isAdding = $state(false);

    // Edit state
    let editingId = $state<string | null>(null);
    let editName = $state("");
    let editUrl = $state("");
    let editApiKey = $state("");
    let showEditApiKey = $state(false);

    // Loading state
    let isLoading = $state(false);

    onMount(async () => {
        await customBackendService.loadBackends();
    });

    async function addBackend() {
        if (!newName.trim() || !newUrl.trim()) return;

        isLoading = true;
        try {
            const input: CreateCustomBackendInput = {
                name: newName.trim(),
                url: newUrl.trim(),
                api_key: newApiKey.trim() || undefined
            };

            const backend = await customBackendService.createBackend(input);
            if (backend) {
                // Reset form
                newName = "";
                newUrl = "";
                newApiKey = "";
                isAdding = false;
            }
        } finally {
            isLoading = false;
        }
    }

    function startEdit(backend: CustomBackend) {
        editingId = backend.id;
        editName = backend.name;
        editUrl = backend.url;
        editApiKey = backend.api_key || "";
        showEditApiKey = false;
    }

    function cancelEdit() {
        editingId = null;
        editName = "";
        editUrl = "";
        editApiKey = "";
        showEditApiKey = false;
    }

    async function saveEdit() {
        if (!editingId || !editName.trim() || !editUrl.trim()) return;

        isLoading = true;
        try {
            await customBackendService.updateBackend({
                id: editingId,
                name: editName.trim(),
                url: editUrl.trim(),
                api_key: editApiKey.trim() || undefined
            });
            cancelEdit();
        } finally {
            isLoading = false;
        }
    }

    async function deleteBackend(id: string) {
        if (!confirm("Are you sure you want to delete this backend?")) return;

        isLoading = true;
        try {
            await customBackendService.deleteBackend(id);
        } finally {
            isLoading = false;
        }
    }
</script>

<div class="container max-w-3xl mx-auto py-8">
    <div class="mb-6">
        <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Models</p>
        <h2 class="text-2xl font-semibold">Custom Backends</h2>
        <p class="text-sm text-muted-foreground/70 mt-1">
            Connect OpenAI-compatible endpoints and manage credentials.
        </p>
    </div>

    <Card.Root class="surface-card border-0 overflow-hidden">
        <Card.Content class="p-6">
            <div class="space-y-6">
                <!-- Existing backends list -->
                {#each customBackendService.backends as backend (backend.id)}
                    <div class="pb-6 border-b surface-divider last:border-0 last:pb-0">
                        {#if editingId === backend.id}
                            <!-- Edit mode -->
                            <div class="space-y-3">
                                <div>
                                    <label class="text-xs font-medium text-muted-foreground mb-1 block">Name</label>
                                    <Input
                                        bind:value={editName}
                                        placeholder="Backend name"
                                        class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                                    />
                                </div>
                                <div>
                                    <label class="text-xs font-medium text-muted-foreground mb-1 block">URL</label>
                                    <Input
                                        bind:value={editUrl}
                                        placeholder="https://api.example.com/v1/chat/completions"
                                        class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                                    />
                                </div>
                                <div>
                                    <label class="text-xs font-medium text-muted-foreground mb-1 block">API Key (optional)</label>
                                    <div class="relative">
                                        <Input
                                            type={showEditApiKey ? "text" : "password"}
                                            bind:value={editApiKey}
                                            placeholder="API key (optional)"
                                            class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                                        />
                                        <button
                                            type="button"
                                            class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                                            onclick={() => showEditApiKey = !showEditApiKey}
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
                            <!-- View mode -->
                            <div class="flex items-start justify-between">
                                <div>
                                    <h3 class="text-sm font-medium">{backend.name}</h3>
                                    <p class="text-xs text-muted-foreground mt-1 font-mono truncate max-w-md">{backend.url}</p>
                                    {#if backend.api_key}
                                        <p class="text-xs text-muted-foreground mt-0.5">API Key configured</p>
                                    {/if}
                                </div>
                                <div class="flex gap-1">
                                    <Button
                                        size="icon"
                                        variant="ghost"
                                        class="h-8 w-8 hover:bg-white/5"
                                        onclick={() => startEdit(backend)}
                                    >
                                        <Edit2 class="h-4 w-4" />
                                    </Button>
                                    <Button
                                        size="icon"
                                        variant="ghost"
                                        class="h-8 w-8 text-destructive hover:text-destructive hover:bg-white/5"
                                        onclick={() => deleteBackend(backend.id)}
                                    >
                                        <Trash2 class="h-4 w-4" />
                                    </Button>
                                </div>
                            </div>
                        {/if}
                    </div>
                {/each}

                <!-- Add new backend form -->
                {#if isAdding}
                    <div class="pt-6 border-t surface-divider space-y-3">
                        <h3 class="text-xs font-medium text-muted-foreground">Add New Backend</h3>
                        <div>
                            <label class="text-xs font-medium text-muted-foreground mb-1 block">Name</label>
                            <Input
                                bind:value={newName}
                                placeholder="My Custom Backend"
                                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                            />
                        </div>
                        <div>
                            <label class="text-xs font-medium text-muted-foreground mb-1 block">URL</label>
                            <Input
                                bind:value={newUrl}
                                placeholder="https://api.example.com/v1/chat/completions"
                                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                            />
                        </div>
                        <div>
                            <label class="text-xs font-medium text-muted-foreground mb-1 block">API Key (optional)</label>
                            <div class="relative">
                                <Input
                                    type={showNewApiKey ? "text" : "password"}
                                    bind:value={newApiKey}
                                    placeholder="API key (optional)"
                                    class="pr-10 glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                                />
                                <button
                                    type="button"
                                    class="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground hover:text-foreground"
                                    onclick={() => showNewApiKey = !showNewApiKey}
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
                                onclick={addBackend}
                                disabled={isLoading || !newName.trim() || !newUrl.trim()}
                            >
                                <Check class="h-4 w-4 mr-1" />
                                Add Backend
                            </Button>
                            <Button
                                size="sm"
                                variant="ghost"
                                onclick={() => {
                                    isAdding = false;
                                    newName = "";
                                    newUrl = "";
                                    newApiKey = "";
                                }}
                            >
                                <X class="h-4 w-4 mr-1" />
                                Cancel
                            </Button>
                        </div>
                    </div>
                {:else}
                    <Button
                        variant="outline"
                        class="w-full glass-panel-minimal border-white/10 hover:glass-light"
                        onclick={() => isAdding = true}
                    >
                        <Plus class="h-4 w-4 mr-2" />
                        Add Custom Backend
                    </Button>
                {/if}
            </div>
        </Card.Content>
    </Card.Root>

    <div class="mt-3 text-[11px] text-muted-foreground/70">
        <p>Custom backends allow you to connect to any OpenAI-compatible API endpoint. The API key is optional and stored securely.</p>
    </div>
</div>
