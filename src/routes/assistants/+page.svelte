<script lang="ts">
    import { Textarea } from "$lib/components/ui/textarea";
    import { Button } from "$lib/components/ui/button";
    import { Input } from "$lib/components/ui/input";
    import { invoke } from "@tauri-apps/api/tauri";
    import { onMount } from "svelte";
    import type { SystemPrompt } from "$lib/types";
    import { Trash2 } from "lucide-svelte";

    let prompts: SystemPrompt[] = [];
    let currentPrompt: string = "";
    let currentName: string = "";
    let selectedPromptId: string | null = null;
    let isLoading = false;

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
        if (!confirm('Are you sure you want to delete this prompt?')) {
            return;
        }

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

<div class="container mx-auto p-4">
    <div class="flex items-center justify-between mb-4">
        <h1 class="text-2xl font-bold">System Prompts</h1>
        {#if selectedPromptId}
            <div class="space-x-2">
                <Button variant="outline" on:click={cancelEdit}>Cancel</Button>
                <Button on:click={savePrompt}>Update Prompt</Button>
            </div>
        {:else}
            <Button on:click={savePrompt}>Save New Prompt</Button>
        {/if}
    </div>

    <div class="grid w-full gap-4">
        <Input
            bind:value={currentName}
            placeholder="Enter prompt name..."
            class="w-full"
        />
        <Textarea 
            bind:value={currentPrompt}
            placeholder="Enter your system prompt here..."
            class="min-h-[200px] resize-y"
        />

        {#if isLoading}
            <div class="text-center">Loading...</div>
        {:else}
            <div class="grid gap-4">
                {#each prompts as prompt (prompt.id)}
                    <div class="border rounded-lg p-4">
                        <div class="flex justify-between items-start mb-2">
                            <div>
                                <h3 class="font-medium">{prompt.name}</h3>
                                <div class="text-sm text-muted-foreground">
                                    Last updated: {new Date(prompt.updated_at).toLocaleString()}
                                </div>
                            </div>
                            <div class="flex gap-2">
                                <Button 
                                    variant="ghost" 
                                    size="sm"
                                    on:click={() => editPrompt(prompt)}
                                >
                                    Edit
                                </Button>
                                <Button 
                                    variant="ghost" 
                                    size="sm"
                                    class="text-destructive hover:text-destructive"
                                    on:click={() => deletePrompt(prompt.id)}
                                >
                                    <Trash2 class="size-4" />
                                </Button>
                            </div>
                        </div>
                        <div class="whitespace-pre-wrap">{prompt.content}</div>
                    </div>
                {/each}
            </div>
        {/if}
    </div>
</div>
