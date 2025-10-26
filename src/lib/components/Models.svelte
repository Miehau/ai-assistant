<script lang="ts">
    import * as Card from "$lib/components/ui/card";
    import ApiKeyInput from "./ApiKeyInput.svelte";
    import { onMount } from "svelte";
    import { modelRegistry } from "$lib/models/registry";
    import { apiKeyService } from "$lib/models";

    // Define the provider type
    type Provider = {
        value: string;
        label: string;
    };

    // Get providers from the registry
    const providers: Provider[] = modelRegistry.getAllProviders().map(p => ({
        value: p.id,
        label: p.name
    }));

    onMount(async () => {
        // Load API keys
        await apiKeyService.loadAllApiKeys();
    });
</script>

<div class="container max-w-2xl mx-auto py-6">
    <h1 class="text-sm font-medium tracking-wide uppercase text-muted-foreground mb-4">API Keys</h1>
    
    <Card.Root class="border-0 overflow-hidden shadow-sm bg-card/50 backdrop-blur-[2px] rounded-xl">
        <Card.Content class="p-5">
            <div class="space-y-5">
                {#each providers as provider}
                    <div class="pb-5 border-b border-border/40 last:border-0 last:pb-0">
                        <h3 class="text-xs font-medium mb-2.5 text-muted-foreground">{provider.label}</h3>
                        <ApiKeyInput {provider} />
                    </div>
                {/each}
            </div>
        </Card.Content>
    </Card.Root>
    
    <div class="mt-3 text-[11px] text-muted-foreground/70">
        <p>API keys are stored securely in your system's credential manager and are only used to authenticate with the respective providers.</p>
    </div>
</div>
