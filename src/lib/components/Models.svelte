<script lang="ts">
    import { Input } from "$lib/components/ui/input";
    import * as Select from "$lib/components/ui/select";
    import { Button } from "$lib/components/ui/button";
    import * as Card from "$lib/components/ui/card";
    import type { Selected } from "bits-ui";
    import { slide } from 'svelte/transition';
    import { cubicOut } from 'svelte/easing';
    import ApiKeyInput from "./ApiKeyInput.svelte";
    import { onMount } from "svelte";
    import { Switch } from "$lib/components/ui/switch";
    import type { Model } from "$lib/types/models";
    import { Trash2, Zap, Eye, Headphones, Database } from "lucide-svelte";
    import { modelService, apiKeyService, modelRegistry } from "$lib/models";

    let models: Model[] = [];
    let registryModels: Model[] = [];

    // Define the options for the Select component
    type Provider = {
        value: string;
        label: string;
    };

    // Get providers from the registry
    const providers: Provider[] = modelRegistry.getAllProviders().map(p => ({
        value: p.id,
        label: p.name
    }));

    let selectedProvider: Provider = providers[0];
    let modelName = "";
    let deploymentName = "";
    let deploymentUrl = "";
    let customUrl = "";

    async function loadModels() {
        try {
            // Load stored models
            models = await modelService.loadModels();
            
            // Load registry models with capabilities
            registryModels = modelService.getAvailableModelsWithCapabilities();
        } catch (error) {
            console.error("Failed to load models:", error);
        }
    }

    onMount(async () => {
        // Load API keys first
        await apiKeyService.loadAllApiKeys();
        // Then load models
        await loadModels();
    });

    $: formData = {
        provider: selectedProvider.value,
        model_name: modelName,
        ...(selectedProvider.value === "azure" && {
            deployment_name: deploymentName,
            url: deploymentUrl
        }),
        ...(selectedProvider.value === "custom" && {
            url: customUrl
        })
    };

    let isSubmitting = false;

    async function handleSubmit(event: Event) {
        event.preventDefault();
        isSubmitting = true;

        try {
            const modelData = {
                ...formData,
                enabled: true // Add the required enabled property
            };
            const success = await modelService.addModel(modelData);
            if (success) {
                await loadModels();
                
                // Reset form
                modelName = "";
                deploymentName = "";
                deploymentUrl = "";
                customUrl = "";
                selectedProvider = providers[0];
            }
        } catch (error: any) {
            console.error(error);
        } finally {
            isSubmitting = false;
        }
    }
    
    function handleFormModelUpdate(v: Selected<string> | undefined) {
        if (v) {
            const provider = providers.find(p => p.value === v.value);
            if (provider) {
                selectedProvider = provider;
            }
        }
    }

    async function toggleModel(model: Model) {
        try {
            const success = await modelService.toggleModel({ 
                provider: model.provider, 
                model_name: model.model_name 
            });
            if (success) {
                await loadModels();  // Refresh the list
            }
        } catch (error) {
            console.error("Failed to toggle model:", error);
        }
    }

    async function deleteModel(model: Model) {
        try {
            const success = await modelService.deleteModel(model);
            if (success) {
                await loadModels();  // Refresh the list
            }
        } catch (error) {
            console.error("Failed to delete model:", error);
        }
    }
    
    // Helper function to get capability icon
    function getCapabilityIcon(capability: string) {
        switch (capability) {
            case 'vision': return Eye;
            case 'audio': return Headphones;
            case 'embedding': return Database;
            case 'function_calling': return Zap;
            default: return null;
        }
    }
</script>

<div class="pt-6">
    <Card.Root class="max-w-2xl mx-auto text-sm">
        <Card.Header class="space-y-0.5">
            <Card.Title class="text-base">Add New Model</Card.Title>
        </Card.Header>
        <Card.Content>
            <form on:submit={handleSubmit} class="space-y-3">
                <div class="space-y-0.5">
                    <div class="flex items-center gap-2">
                        <label for="name" class="font-medium w-20 text-xs">Name</label>
                        <div class="w-full max-w-xl">
                            <Input id="name" bind:value={modelName} class="w-full text-sm h-8" placeholder="Model name, e.g. gpt-4" />
                        </div>
                    </div>
                </div>

                <div class="space-y-0.5">
                    <div class="flex items-center gap-2">
                        <label for="provider" class="font-medium w-20 text-xs">Provider</label>
                        <Select.Root 
                            selected={selectedProvider}
                            onSelectedChange={handleFormModelUpdate}
                            className="w-full max-w-sm text-sm">
                            <Select.Trigger class="w-full h-8">
                                <Select.Value placeholder="Select provider" />
                            </Select.Trigger>
                            <Select.Content>
                                {#each providers as provider}
                                    <Select.Item value={provider}>
                                        {provider.label}
                                    </Select.Item>
                                {/each}
                            </Select.Content>
                        </Select.Root>
                    </div>
                </div>

                {#if selectedProvider.value === "azure"}
                    <div transition:slide={{ duration: 300, easing: cubicOut }}>
                        <div class="space-y-1.5">
                            <div class="flex items-center space-x-2">
                                <label for="deploymentName" class="font-medium w-24">Deployment Name:</label>
                                <Input id="deploymentName" bind:value={deploymentName} />
                            </div>
                        </div>

                        <div class="space-y-1.5">
                            <div class="flex items-center space-x-2">
                                <label for="deploymentUrl" class="font-medium w-24">Deployment URL:</label>
                                <Input id="deploymentUrl" bind:value={deploymentUrl} />
                            </div>
                        </div>
                    </div>
                {/if}

                {#if selectedProvider.value === "custom"}
                    <div transition:slide={{ duration: 300, easing: cubicOut }}>
                        <div class="space-y-1.5">
                            <div class="flex items-center space-x-2">
                                <label for="customUrl" class="font-medium w-24">API URL:</label>
                                <Input id="customUrl" bind:value={customUrl} />
                            </div>
                            <p class="text-sm text-muted-foreground">
                                The URL of your custom API endpoint
                            </p>
                        </div>
                    </div>
                {/if}
            </form>
        </Card.Content>
        <Card.Footer class="flex justify-end gap-2">
            <Button variant="outline" size="sm" class="h-7 text-xs">Cancel</Button>
            <Button type="submit" disabled={isSubmitting} on:click={handleSubmit} size="sm" class="h-7 text-xs">
                {isSubmitting ? "Submitting..." : "Submit"}
            </Button>
        </Card.Footer>
    </Card.Root>

    <Card.Root class="mt-4 max-w-2xl mx-auto text-sm">
        <Card.Header class="space-y-0.5">
            <Card.Title class="text-base">Configured Models</Card.Title>
        </Card.Header>
        <Card.Content>
            {#if models.length === 0}
                <p class="text-xs text-muted-foreground">No models configured yet.</p>
            {:else}
                <div class="space-y-1">
                    {#each models as model}
                        <div class="flex items-center justify-between p-1.5 border rounded-lg hover:bg-muted/50 transition-colors">
                            <div class="flex items-center gap-1.5">
                                <h4 class="font-medium text-sm">{model.model_name}</h4>
                                <span class="text-xs text-muted-foreground">•</span>
                                <span class="text-xs text-muted-foreground">{model.provider}</span>
                            </div>
                            <div class="flex items-center gap-1.5">
                                <Switch 
                                    checked={model.enabled} 
                                    onCheckedChange={() => toggleModel(model)}
                                    class="scale-90"
                                />
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    class="text-destructive hover:text-destructive/90 h-7 w-7"
                                    on:click={() => deleteModel(model)}
                                    aria-label={`Delete model ${model.model_name}`}
                                >
                                    <Trash2 class="h-3.5 w-3.5" />
                                </Button>
                            </div>
                        </div>
                    {/each}
                </div>
            {/if}
        </Card.Content>
    </Card.Root>

    <Card.Root class="mt-4 max-w-2xl mx-auto text-sm">
        <Card.Header class="space-y-0.5">
            <Card.Title class="text-base">API Keys</Card.Title>
        </Card.Header>
        <Card.Content>
            {#each providers as provider}
                <ApiKeyInput {provider} />
            {/each}
        </Card.Content>
    </Card.Root>
</div>
