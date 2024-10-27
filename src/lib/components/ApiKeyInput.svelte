<script lang="ts">
    import { Input } from "$lib/components/ui/input";
    import { Button } from "$lib/components/ui/button";
    import { cn } from "$lib/utils";
    import { invoke } from "@tauri-apps/api/tauri";

    export let provider: { value: string; label: string };

    let apiKey = "";
    let isApiKeyFocused = false;
    let isApiKeyHovered = false;

    $: showApiKey = isApiKeyFocused || isApiKeyHovered;

    async function submitApiKey() {
        try {
            await invoke<string>("set_api_key", { provider: provider.value, apiKey });
            console.log(`API key for ${provider.label} updated successfully`);
            // Optionally, add some user feedback here
        } catch (error) {
            console.error(`Error setting API key for ${provider.label}:`, error);
        }
    }
</script>

<div class="space-y-1.5 mb-4">
    <div class="flex items-center space-x-2">
        <label for={`apiKey-${provider.value}`} class="font-medium w-24">{provider.label} API Key:</label>
        <div class="relative flex-grow" role="none"
             on:mouseenter={() => isApiKeyHovered = true}
             on:mouseleave={() => isApiKeyHovered = false}>
            <div class="relative">
                <Input id={`apiKey-${provider.value}`}
                       type="password"
                       class={cn(
                           "pr-20 transition-all duration-200",
                           !showApiKey && "filter blur-sm"
                       )}
                       bind:value={apiKey} 
                       on:focus={() => isApiKeyFocused = true}
                       on:blur={() => isApiKeyFocused = false}
                       placeholder={`Enter ${provider.label} API Key`}
                       aria-label={`${provider.label} API Key`}
                       aria-describedby={`apiKeyDescription-${provider.value}`} />
                <Button
                    type="submit"
                    class="absolute inset-y-0 right-0 px-3 flex items-center text-sm focus:outline-none"
                    on:click={submitApiKey}
                    aria-label={`Submit ${provider.label} API Key`}
                >
                    Submit
                </Button>
            </div>
        </div>
    </div>
    <p id={`apiKeyDescription-${provider.value}`} class="text-sm text-muted-foreground">
        Hover over or focus on the input to reveal the API key
    </p>
</div>
