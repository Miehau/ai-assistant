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

<div class="container max-w-3xl mx-auto py-8">
    <div class="mb-6">
        <p class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Models</p>
        <h1 class="text-2xl font-semibold">API Keys</h1>
        <p class="text-sm text-muted-foreground/70 mt-1">
            Securely connect providers with per-model credentials.
        </p>
    </div>
    
    <Card.Root class="surface-card border-0 overflow-hidden">
        <Card.Content class="p-6">
            <div class="space-y-6">
                {#each providers as provider}
                    <div class="pb-6 border-b surface-divider last:border-0 last:pb-0">
                        <h3 class="text-xs font-medium mb-3 text-muted-foreground/80">{provider.label}</h3>
                        <ApiKeyInput {provider} />
                    </div>
                {/each}
            </div>
        </Card.Content>
    </Card.Root>
    
    <div class="mt-3 text-[11px] text-muted-foreground/70">
        <p>Keys are stored in your system credential manager and never leave your device.</p>
    </div>
</div>
