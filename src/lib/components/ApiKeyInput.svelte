<script lang="ts">
    import { Input } from "$lib/components/ui/input";
    import { Button } from "$lib/components/ui/button";
    import { cn } from "$lib/utils";
    import { onMount } from "svelte";
    import { Trash2 } from "lucide-svelte";
    import { apiKeyService } from "$lib/models";
    import { loadModels } from "$lib/components/chat/store";

    let { provider }: { provider: { value: string; label: string } } = $props();

    let apiKey = $state("");
    let isApiKeyFocused = $state(false);
    let isApiKeyHovered = $state(false);
    let isLoading = $state(false);

    let showApiKey = $derived(isApiKeyFocused || isApiKeyHovered);

    // Sync local state with service's reactive state
    $effect(() => {
        const serviceKey = apiKeyService.apiKeys[provider.value];
        if (serviceKey !== undefined) {
            apiKey = serviceKey;
        }
    });

    onMount(async () => {
        try {
            const savedKey = await apiKeyService.getApiKey(provider.value);
            if (savedKey) {
                apiKey = savedKey;
            }
        } catch (error) {
            console.error(`Error loading API key for ${provider.label}:`, error);
        }
    });

    async function submitApiKey() {
        isLoading = true;
        try {
            const success = await apiKeyService.setApiKey(provider.value, apiKey);
            if (success) {
                console.log(`API key for ${provider.label} updated successfully`);
                // Reload models to reflect API key changes in model selector
                await loadModels();
            }
        } catch (error) {
            console.error(`Error setting API key for ${provider.label}:`, error);
        } finally {
            isLoading = false;
        }
    }

    async function deleteApiKey() {
        isLoading = true;
        try {
            const success = await apiKeyService.deleteApiKey(provider.value);
            if (success) {
                apiKey = "";
                console.log(`API key for ${provider.label} deleted successfully`);
                // Reload models to reflect API key changes in model selector
                await loadModels();
            }
        } catch (error) {
            console.error(`Error deleting API key for ${provider.label}:`, error);
        } finally {
            isLoading = false;
        }
    }
</script>

<div class="space-y-1.5 mb-4">
    <div class="flex items-center space-x-2">
        <label for={`apiKey-${provider.value}`} class="font-medium w-24">{provider.label} API Key:</label>
        <div class="relative flex-grow">
            <div class="relative"
                 role="button"
                 tabindex="0"
                 onmouseenter={() => isApiKeyHovered = true}
                 onmouseleave={() => isApiKeyHovered = false}>
                <Input id={`apiKey-${provider.value}`}
                       class={cn(
                           "pr-[164px] transition-all duration-200",
                           !showApiKey && "filter blur-sm"
                       )}
                       bind:value={apiKey}
                       onfocus={() => isApiKeyFocused = true}
                       onblur={() => isApiKeyFocused = false}
                       placeholder={`Enter ${provider.label} API Key`}
                       aria-label={`${provider.label} API Key`}
                       aria-describedby={`apiKeyDescription-${provider.value}`} />
            </div>
            <div class="absolute inset-y-0 right-0 flex items-center">
                <div class="flex">
                    <Button
                        type="submit"
                        class="rounded-r-none"
                        onclick={submitApiKey}
                        aria-label={`Submit ${provider.label} API Key`}
                    >
                        Submit
                    </Button>
                    <Button
                        type="button"
                        variant="destructive"
                        class="rounded-l-none border-l-0"
                        onclick={deleteApiKey}
                        aria-label={`Delete ${provider.label} API Key`}
                    >
                        <Trash2 class="h-4 w-4" />
                    </Button>
                </div>
            </div>
        </div>
    </div>
</div>
