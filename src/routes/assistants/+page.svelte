<script lang="ts">
    import { Textarea } from "$lib/components/ui/textarea";
    import { Button } from "$lib/components/ui/button";
    import { Input } from "$lib/components/ui/input";
    import { invoke } from "@tauri-apps/api/tauri";
    import { onMount } from "svelte";
    import type { SystemPrompt } from "$lib/types";
    import { Trash2 } from "lucide-svelte";

    let prompts = $state<SystemPrompt[]>([]);
    let currentPrompt = $state("");
    let currentName = $state("");
    let selectedPromptId = $state<string | null>(null);
    let isLoading = $state(false);

    async function loadPrompts() {
        try {
            isLoading = true;
            prompts = await invoke('get_all_system_prompts');
        } catch (error) {
            console.error('Error loading prompts:', error);
        } finally {
            isLoading = false;
        }
    }

    async function savePrompt() {
        if (!currentPrompt.trim() || !currentName.trim()) {
            alert('Please enter both name and prompt');
            return;
        }

        try {
            isLoading = true;
            if (selectedPromptId) {
                await invoke('update_system_prompt', {
                    id: selectedPromptId,
                    name: currentName,
                    content: currentPrompt
                });
            } else {
                await invoke('save_system_prompt', {
                    name: currentName,
                    content: currentPrompt
                });
            }
            await loadPrompts();
            currentPrompt = "";
            currentName = "";
            selectedPromptId = null;
        } catch (error) {
            console.error('Error saving prompt:', error);
        } finally {
            isLoading = false;
        }
    }

    function editPrompt(prompt: SystemPrompt) {
        currentPrompt = prompt.content;
        currentName = prompt.name;
        selectedPromptId = prompt.id;
    }

    function cancelEdit() {
        currentPrompt = "";
        currentName = "";
        selectedPromptId = null;
    }

    async function deletePrompt(id: string) {
        try {
            isLoading = true;
            await invoke('delete_system_prompt', { id });
            await loadPrompts();
            if (selectedPromptId === id) {
                cancelEdit();
            }
        } catch (error) {
            console.error('Error deleting prompt:', error);
        } finally {
            isLoading = false;
        }
    }

    onMount(loadPrompts);
</script>

<div class="pt-8">
    <div class="max-w-4xl mx-auto text-sm">
        <div class="mb-6">
            <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Assistants</p>
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-2xl font-semibold">System prompts</h1>
                    <p class="text-sm text-muted-foreground/70 mt-1">Create and reuse prompt templates across conversations.</p>
                </div>
                {#if selectedPromptId}
                    <div class="space-x-2">
                        <Button variant="outline" size="sm" class="h-8 text-xs border-white/10 hover:bg-white/5" onclick={cancelEdit}>Cancel</Button>
                        <Button size="sm" class="h-8 text-xs bg-white/10 border-white/15" onclick={savePrompt}>Update Prompt</Button>
                    </div>
                {:else}
                    <Button size="sm" class="h-8 text-xs bg-white/10 border-white/15" onclick={savePrompt}>Save New Prompt</Button>
                {/if}
            </div>
        </div>

        <div class="grid w-full gap-4 surface-card p-6">
            <div class="grid gap-3">
                <Input
                    bind:value={currentName}
                    placeholder="Prompt name"
                    class="w-full h-9 text-sm glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                />
                <Textarea
                    bind:value={currentPrompt}
                    placeholder="Write your system prompt..."
                    class="min-h-[180px] resize-y text-sm glass-panel-minimal border-white/10 focus-within:ring-1 focus-within:ring-white/15"
                />
            </div>

            {#if isLoading}
                <div class="text-center text-xs text-muted-foreground">Loading...</div>
            {:else}
                <div class="grid gap-3">
                    {#each prompts as prompt (prompt.id)}
                        <div class="surface-card-quiet p-3 transition-all duration-200 hover:bg-white/5">
                            <div class="flex justify-between items-start gap-4">
                                <div>
                                    <h3 class="text-sm font-medium">{prompt.name}</h3>
                                    <div class="text-xs text-muted-foreground/70">
                                        Last updated: {new Date(prompt.updated_at).toLocaleString()}
                                    </div>
                                    <p class="mt-1 text-xs text-muted-foreground/70">
                                        {prompt.content.split('.')[0]}
                                        {prompt.content.split('.')[1] ? '.' + prompt.content.split('.')[1] + '...' : '...'}
                                    </p>
                                </div>
                                <div class="flex gap-1.5">
                                    <Button
                                        variant="ghost"
                                        size="sm"
                                        class="text-xs hover:bg-white/5"
                                        onclick={() => editPrompt(prompt)}
                                    >
                                        Edit
                                    </Button>
                                    <Button
                                        variant="ghost"
                                        size="sm"
                                        class="text-destructive hover:text-destructive hover:bg-white/5"
                                        onclick={() => deletePrompt(prompt.id)}
                                    >
                                        <Trash2 class="size-4" />
                                    </Button>
                                </div>
                            </div>
                        </div>
                    {/each}
                </div>
            {/if}
        </div>
    </div>
</div>
