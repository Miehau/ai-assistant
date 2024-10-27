<script lang="ts">
    import { browser } from "$app/environment";
    import { Input } from "$lib/components/ui/input";
    import * as Select from "$lib/components/ui/select";
    import { Button } from "$lib/components/ui/button";
    import * as Card from "$lib/components/ui/card";
    import { invoke } from "@tauri-apps/api/tauri";
    import type { Selected } from "bits-ui";
    import { slide } from 'svelte/transition';
    import { cubicOut } from 'svelte/easing';
    import ApiKeyInput from "./ApiKeyInput.svelte";

    // Define the options for the Select component
    const providers = [
        { value: "openai", label: "OpenAI" },
        { value: "anthropic", label: "Anthropic" },
        { value: "azure", label: "Azure" },
    ];

    let selectedProvider = providers[0];
    let modelName = "";
    let deploymentName = "";
    let deploymentUrl = "";

    $: formData = {
        name: modelName,
        provider: selectedProvider.value,
        deploymentName: deploymentName,
        deploymentUrl: deploymentUrl,
    };
    let isSubmitting = false;

    // Handle form submission
    async function handleSubmit(event: Event) {
        event.preventDefault();
        isSubmitting = true;

        try {
            // Invoke the Tauri command to handle form submission
            const response = await invoke<string>("add_model", formData);

            // Reset the form
            formData = { name: "", provider: "openai", deploymentName: "", deploymentUrl: "" };
            selectedProvider = providers[0];
        } catch (error: any) {
            console.error(error);
        } finally {
            isSubmitting = false;
        }
    }

    function handleFormModelUpdate(v: Selected<typeof providers[number]>) {
        if (v) selectedProvider = v;
    }
</script>

<Card.Root>
    <Card.Header>
        <Card.Title>Add New Model</Card.Title>
    </Card.Header>
    <Card.Content>
        <form on:submit={handleSubmit} class="space-y-6">
            <div class="space-y-1.5">
                <div class="flex items-center space-x-2">
                    <label for="name" class="font-medium w-24">Name:</label>
                    <Input id="name" bind:value={modelName} />
                </div>
                <p class="text-sm text-muted-foreground">
                    Model name, e.g. gpt-4
                </p>
            </div>

            <div class="space-y-1.5">
                <div class="flex items-center space-x-2">
                    <label for="provider" class="font-medium w-24">Provider</label>
                    <Select.Root 
                    selected={selectedProvider}
                    onSelectedChange={handleFormModelUpdate}>
                        <Select.Trigger class="w-full">
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
        </form>
    </Card.Content>
    <Card.Footer class="flex justify-between">
        <Button variant="outline">Cancel</Button>
        <Button type="submit" disabled={isSubmitting} on:click={handleSubmit}>
            {isSubmitting ? "Submitting..." : "Submit"}
        </Button>
    </Card.Footer>
</Card.Root>

<Card.Root class="mt-6">
    <Card.Header>
        <Card.Title>API Keys</Card.Title>
    </Card.Header>
    <Card.Content>
        {#each providers as provider}
            <ApiKeyInput {provider} />
        {/each}
    </Card.Content>
</Card.Root>

<!-- Debugging Output (Optional) -->
{#if browser}
    <pre class="mt-4 p-4 bg-muted rounded-md">{JSON.stringify(formData, null, 2)}</pre>
{/if}
