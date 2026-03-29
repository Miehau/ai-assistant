<script lang="ts">
    import { Input } from "$lib/components/ui/input";
    import { Button } from "$lib/components/ui/button";
    import * as Card from "$lib/components/ui/card";
    import { onMount } from "svelte";
    import { Trash2, Plus, Check, X } from "lucide-svelte";
    import { getHttpBackend, type ModelInfo } from "$lib/backend/http-client";
    import { loadModels } from "$lib/stores/chat";

    const PROVIDERS = [
        { value: "openrouter", label: "OpenRouter" },
        { value: "anthropic", label: "Anthropic" },
        { value: "openai", label: "OpenAI" },
        { value: "ollama", label: "Ollama" },
    ];

    // Popular OpenRouter model IDs as datalist suggestions
    const OPENROUTER_SUGGESTIONS = [
        "meta-llama/llama-3.3-70b-instruct",
        "meta-llama/llama-3.1-8b-instruct",
        "google/gemini-2.0-flash-001",
        "google/gemini-2.5-pro-preview",
        "mistralai/mistral-small-3.1-24b-instruct",
        "mistralai/mistral-nemo",
        "qwen/qwen-2.5-72b-instruct",
        "deepseek/deepseek-r1",
        "deepseek/deepseek-chat-v3-0324",
        "x-ai/grok-3-mini-beta",
    ];

    let models = $state<ModelInfo[]>([]);
    let isLoading = $state(false);
    let isAdding = $state(false);

    let newProvider = $state("openrouter");
    let newModelName = $state("");
    let newDisplayName = $state("");

    onMount(async () => {
        await fetchModels();
    });

    async function fetchModels() {
        try {
            models = await getHttpBackend().listModels();
        } catch (e) {
            console.error("[ServerModels] Failed to fetch models:", e);
        }
    }

    async function addModel() {
        if (!newModelName.trim()) return;
        isLoading = true;
        try {
            await getHttpBackend().addModel(
                newProvider,
                newModelName.trim(),
                newDisplayName.trim() || undefined,
            );
            await fetchModels();
            await loadModels({ force: true });
            newModelName = "";
            newDisplayName = "";
            isAdding = false;
        } catch (e) {
            console.error("[ServerModels] Failed to add model:", e);
        } finally {
            isLoading = false;
        }
    }

    async function deleteModel(id: string) {
        isLoading = true;
        try {
            await getHttpBackend().deleteModel(id);
            await fetchModels();
            await loadModels({ force: true });
        } catch (e) {
            console.error("[ServerModels] Failed to delete model:", e);
        } finally {
            isLoading = false;
        }
    }

    function providerLabel(provider: string) {
        return PROVIDERS.find(p => p.value === provider)?.label ?? provider;
    }
</script>

<div class="container max-w-3xl mx-auto py-8">
    <div class="mb-6">
        <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Server</p>
        <h2 class="text-2xl font-semibold">Server Models</h2>
        <p class="text-sm text-muted-foreground/70 mt-1">
            Models available through the connected server backend.
        </p>
    </div>

    <Card.Root class="surface-card border-0 overflow-hidden">
        <Card.Content class="p-6">
            <div class="space-y-3">
                {#if models.length === 0 && !isAdding}
                    <p class="text-xs text-muted-foreground/60 py-2">No models registered yet.</p>
                {/if}

                {#each models as model (model.id)}
                    <div class="flex items-center justify-between py-2 border-b surface-divider last:border-0">
                        <div>
                            <span class="text-sm font-medium font-mono">{model.name}</span>
                            <span class="ml-2 text-[10px] uppercase tracking-wide text-muted-foreground/60">{providerLabel(model.provider)}</span>
                            {#if model.displayName && model.displayName !== model.name}
                                <p class="text-xs text-muted-foreground/50 mt-0.5">{model.displayName}</p>
                            {/if}
                        </div>
                        <Button
                            size="icon"
                            variant="ghost"
                            class="h-8 w-8 text-destructive hover:text-destructive hover:bg-white/5 shrink-0"
                            onclick={() => deleteModel(model.id!)}
                            disabled={isLoading}
                        >
                            <Trash2 class="h-4 w-4" />
                        </Button>
                    </div>
                {/each}

                {#if isAdding}
                    <div class="pt-4 border-t surface-divider space-y-3">
                        <h3 class="text-xs font-medium text-muted-foreground">Add Model</h3>

                        <div>
                            <label class="text-xs font-medium text-muted-foreground mb-1 block" for="new-model-provider">
                                Provider
                            </label>
                            <select
                                id="new-model-provider"
                                bind:value={newProvider}
                                class="w-full h-9 rounded-md border border-white/10 bg-transparent px-3 text-sm focus:outline-none focus:ring-1 focus:ring-white/15"
                            >
                                {#each PROVIDERS as p}
                                    <option value={p.value}>{p.label}</option>
                                {/each}
                            </select>
                        </div>

                        <div>
                            <label class="text-xs font-medium text-muted-foreground mb-1 block" for="new-model-name">
                                Model ID
                            </label>
                            <Input
                                id="new-model-name"
                                list="openrouter-suggestions"
                                bind:value={newModelName}
                                placeholder={newProvider === "openrouter" ? "meta-llama/llama-3.3-70b-instruct" : "model-id"}
                                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15 font-mono text-xs"
                            />
                            {#if newProvider === "openrouter"}
                                <datalist id="openrouter-suggestions">
                                    {#each OPENROUTER_SUGGESTIONS as s}
                                        <option value={s}></option>
                                    {/each}
                                </datalist>
                            {/if}
                        </div>

                        <div>
                            <label class="text-xs font-medium text-muted-foreground mb-1 block" for="new-model-display">
                                Display name <span class="text-muted-foreground/50">(optional)</span>
                            </label>
                            <Input
                                id="new-model-display"
                                bind:value={newDisplayName}
                                placeholder="Llama 3.3 70B"
                                class="glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                            />
                        </div>

                        <div class="flex gap-2">
                            <Button
                                size="sm"
                                class="glass-badge hover:glass-light"
                                onclick={addModel}
                                disabled={isLoading || !newModelName.trim()}
                            >
                                <Check class="h-4 w-4 mr-1" />
                                Add Model
                            </Button>
                            <Button
                                size="sm"
                                variant="ghost"
                                onclick={() => { isAdding = false; newModelName = ""; newDisplayName = ""; }}
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
                        Add Model
                    </Button>
                {/if}
            </div>
        </Card.Content>
    </Card.Root>

    <div class="mt-3 text-[11px] text-muted-foreground/70">
        <p>Models are stored on the server and available to all sessions. Set provider API keys in <code class="font-mono">server/.env</code>.</p>
    </div>
</div>
