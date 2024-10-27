<script lang="ts">
    import { Input } from "$lib/components/ui/input";
    import { Button } from "$lib/components/ui/button";
    import { cn } from "$lib/utils";
    import { invoke } from "@tauri-apps/api/tauri";
    import { onMount } from "svelte";
    import { Trash2 } from "lucide-svelte";

    export let provider: { value: string; label: string };

    let apiKey = "";
    let isApiKeyFocused = false;
    let isApiKeyHovered = false;

    $: showApiKey = isApiKeyFocused || isApiKeyHovered;

    onMount(async () => {
        try {
            const savedKey = await invoke<string | null>("get_api_key", { provider: provider.value });
            if (savedKey) {
                apiKey = savedKey;
            }
        } catch (error) {
            console.error(`Error loading API key for ${provider.label}:`, error);
        }
    });

    async function submitApiKey() {
        try {
            await invoke<string>("set_api_key", { provider: provider.value, apiKey });
            console.log(`API key for ${provider.label} updated successfully`);
        } catch (error) {
            console.error(`Error setting API key for ${provider.label}:`, error);
        }
    }

    async function deleteApiKey() {
        try {
            await invoke<void>("delete_api_key", { provider: provider.value });
            apiKey = "";
            console.log(`API key for ${provider.label} deleted successfully`);
        } catch (error) {
            console.error(`Error deleting API key for ${provider.label}:`, error);
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
                 on:mouseenter={() => isApiKeyHovered = true}
                 on:mouseleave={() => isApiKeyHovered = false}>
                <Input id={`apiKey-${provider.value}`}
                       class={cn(
                           "pr-[164px] transition-all duration-200",
                           !showApiKey && "filter blur-sm"
                       )}
                       bind:value={apiKey} 
                       on:focus={() => isApiKeyFocused = true}
                       on:blur={() => isApiKeyFocused = false}
                       placeholder={`Enter ${provider.label} API Key`}
                       aria-label={`${provider.label} API Key`}
                       aria-describedby={`apiKeyDescription-${provider.value}`} />
            </div>
            <div class="absolute inset-y-0 right-0 flex items-center">
                <div class="flex">
                    <Button
                        type="submit"
                        class="rounded-r-none"
                        on:click={submitApiKey}
                        aria-label={`Submit ${provider.label} API Key`}
                    >
                        Submit
                    </Button>
                    <Button
                        type="button"
                        variant="destructive"
                        class="rounded-l-none border-l-0"
                        on:click={deleteApiKey}
                        aria-label={`Delete ${provider.label} API Key`}
                    >
                        <Trash2 class="h-4 w-4" />
                    </Button>
                </div>
            </div>
        </div>
    </div>
</div>
