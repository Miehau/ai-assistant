<script lang="ts">
    import * as Tooltip from "$lib/components/ui/tooltip";
    import { Button } from "$lib/components/ui/button";
    import * as Select from "$lib/components/ui/select";
    import type { Model } from "$lib/types/models";
    import type { ModelWithBackend } from "./store";
    import type { SystemPrompt, Message } from "$lib/types";
    import { Eye, Headphones, Database, Brain } from "lucide-svelte";
    import TokenCounter from "./TokenCounter.svelte";

    interface Props {
        availableModels?: Model[];
        systemPrompts?: SystemPrompt[];
        selectedModel: string;
        selectedSystemPrompt?: SystemPrompt | null;
        streamingEnabled?: boolean;
        conversationId?: string;
        currentMessage?: string;
        messages?: Message[];
        isLoading?: boolean;
        onSystemPromptSelect?: (prompt: SystemPrompt) => void;
        onToggleStreaming?: (enabled: boolean) => void;
        onRemoveMessages?: () => void;
        onModelSelect?: (modelName: string) => void;
    }

    let {
        availableModels = [],
        systemPrompts = [],
        selectedModel = $bindable(),
        selectedSystemPrompt = $bindable(null),
        streamingEnabled = $bindable(true),
        conversationId = undefined,
        currentMessage = '',
        messages = [],
        isLoading = false,
        onSystemPromptSelect,
        onToggleStreaming,
        onRemoveMessages,
        onModelSelect
    }: Props = $props();

    function selectSystemPrompt(prompt: SystemPrompt) {
        selectedSystemPrompt = prompt;
        onSystemPromptSelect?.(prompt);
    }

    function toggleStreaming() {
        streamingEnabled = !streamingEnabled;
        onToggleStreaming?.(streamingEnabled);
    }

    function removeMessages() {
        onRemoveMessages?.();
    }

    // Track if model dropdown is open for lazy-loading tooltips
    let modelDropdownOpen = $state(false);

    // Get the display provider name for a model
    // Custom models use their backend name, others use their provider
    function getProviderLabel(model: Model): string {
        const modelWithBackend = model as ModelWithBackend;
        if (model.provider === 'custom' && modelWithBackend.backendName) {
            return modelWithBackend.backendName;
        }
        return model.provider;
    }

    // Memoized provider groups - only recalculates when availableModels changes
    const providerGroups = $derived.by(() => {
        const groups: { provider: string; models: Model[] }[] = [];

        // Group models by provider (using backend name for custom models)
        availableModels.forEach((model) => {
            const providerLabel = getProviderLabel(model);
            const existingGroup = groups.find(
                (g) => g.provider === providerLabel,
            );
            if (existingGroup) {
                existingGroup.models.push(model);
            } else {
                groups.push({
                    provider: providerLabel,
                    models: [model],
                });
            }
        });

        // Sort groups alphabetically by provider name
        groups.sort((a, b) => a.provider.localeCompare(b.provider));

        // Sort models within each group by name
        groups.forEach((group) => {
            group.models.sort((a, b) =>
                a.model_name.localeCompare(b.model_name),
            );
        });

        return groups;
    });
</script>

<div class="flex items-center justify-between w-full gap-2">
    <!-- Left section: Selectors -->
    <div class="flex items-center gap-2 flex-1">
        <Select.Root
        selected={{
            value: selectedSystemPrompt?.id ?? "",
            label: selectedSystemPrompt?.name ?? "",
        }}
        onSelectedChange={(v) => {
            if (v) {
                const prompt = systemPrompts.find((p) => p.id === v.value);
                if (prompt) selectSystemPrompt(prompt);
            }
        }}
    >
        <Select.Trigger class="min-w-[150px] w-fit">
            {#if selectedSystemPrompt}
                <div class="flex items-center gap-2">
                    <span class="truncate max-w-[120px]"
                        >{selectedSystemPrompt.name}</span
                    >
                </div>
            {:else}
                <span class="text-muted-foreground">Select system prompt</span>
            {/if}
        </Select.Trigger>
        <Select.Portal>
            <Select.Content>
                <Select.ScrollUpButton />
                <Select.Viewport class="max-h-[400px]">
                    {#if systemPrompts.length === 0}
                        <Select.Item value="" disabled
                            >No system prompts available</Select.Item
                        >
                    {:else}
                        {#each systemPrompts as prompt}
                            <Select.Item value={prompt.id}>
                                <div class="flex items-center gap-2">
                                    <span>{prompt.name}</span>
                                </div>
                            </Select.Item>
                        {/each}
                    {/if}
                </Select.Viewport>
                <Select.ScrollDownButton />
            </Select.Content>
        </Select.Portal>
    </Select.Root>

    <Select.Root
        type="single"
        bind:value={selectedModel}
        onValueChange={(v) => {
            if (v) {
                onModelSelect?.(v);
            }
        }}
        onOpenChange={(open) => {
            modelDropdownOpen = open;
        }}
    >
        <Select.Trigger class="min-w-[150px] max-w-[220px] w-fit justify-between">
            {#if selectedModel}
                {@const model = availableModels.find(m => m.model_name === selectedModel)}
                <span class="truncate">{model ? `${model.model_name} • ${getProviderLabel(model)}` : selectedModel}</span>
            {:else}
                <span class="text-muted-foreground">Select a model</span>
            {/if}
        </Select.Trigger>
        <Select.Portal>
            <Select.Content class="w-[350px] text-xs">
                <Select.ScrollUpButton />
                <Select.Viewport class="max-h-[400px]">
                    {#if availableModels.length === 0}
                        <Select.Item value="" disabled
                            >No models available</Select.Item
                        >
                    {:else}
                        <!-- Group models by provider -->
                        {#each providerGroups as group}
                            <div
                                class="px-2 py-1.5 text-xs font-semibold text-muted-foreground bg-muted/50"
                            >
                                {group.provider}
                            </div>

                            {#each group.models as model}
                                <Select.Item
                                    value={model.model_name}
                                    class="py-2"
                                >
                                    <div class="flex flex-col gap-1 w-full">
                                        <div
                                            class="grid grid-cols-[1fr_auto] items-center w-full"
                                        >
                                            <span class="font-medium text-xs"
                                                >{model.name ||
                                                    model.model_name}</span
                                            >

                                            {#if model.capabilities}
                                                <div
                                                    class="flex gap-1 justify-self-end"
                                                >
                                                    {#if modelDropdownOpen}
                                                        {#if model.capabilities.reasoning}
                                                            <Tooltip.Root>
                                                                <Tooltip.Trigger
                                                                    asChild
                                                                >
                                                                    {#snippet child({ props })}
                                                                        <span
                                                                            {...props}
                                                                            class="cursor-help inline-block"
                                                                        >
                                                                            <Brain
                                                                                class="h-3 w-3 text-accent-amber drop-shadow-[0_0_4px_rgba(251,191,36,0.5)]"
                                                                            />
                                                                        </span>
                                                                    {/snippet}
                                                                </Tooltip.Trigger>
                                                                <Tooltip.Content
                                                                    side="top"
                                                                >
                                                                    Advanced
                                                                    reasoning
                                                                </Tooltip.Content>
                                                            </Tooltip.Root>
                                                        {/if}
                                                        {#if model.capabilities.vision}
                                                            <Tooltip.Root>
                                                                <Tooltip.Trigger
                                                                    asChild
                                                                >
                                                                    {#snippet child({ props })}
                                                                        <span
                                                                            {...props}
                                                                            class="cursor-help inline-block"
                                                                        >
                                                                            <Eye
                                                                                class="h-3 w-3 text-accent-cyan drop-shadow-[0_0_4px_rgba(34,211,238,0.5)]"
                                                                            />
                                                                        </span>
                                                                    {/snippet}
                                                                </Tooltip.Trigger>
                                                                <Tooltip.Content
                                                                    side="top"
                                                                >
                                                                    Vision
                                                                    capability
                                                                </Tooltip.Content>
                                                            </Tooltip.Root>
                                                        {/if}
                                                        {#if model.capabilities.audio}
                                                            <Tooltip.Root>
                                                                <Tooltip.Trigger
                                                                    asChild
                                                                >
                                                                    {#snippet child({ props })}
                                                                        <span
                                                                            {...props}
                                                                            class="cursor-help inline-block"
                                                                        >
                                                                            <Headphones
                                                                                class="h-3 w-3 text-primary drop-shadow-[0_0_4px_rgba(82,183,136,0.5)]"
                                                                            />
                                                                        </span>
                                                                    {/snippet}
                                                                </Tooltip.Trigger>
                                                                <Tooltip.Content
                                                                    side="top"
                                                                >
                                                                    Audio capability
                                                                </Tooltip.Content>
                                                            </Tooltip.Root>
                                                        {/if}
                                                        {#if model.capabilities.embedding}
                                                            <Tooltip.Root>
                                                                <Tooltip.Trigger
                                                                    asChild
                                                                >
                                                                    {#snippet child({ props })}
                                                                        <span
                                                                            {...props}
                                                                            class="cursor-help inline-block"
                                                                        >
                                                                            <Database
                                                                                class="h-3 w-3 text-accent-purple drop-shadow-[0_0_4px_rgba(168,85,247,0.5)]"
                                                                            />
                                                                        </span>
                                                                    {/snippet}
                                                                </Tooltip.Trigger>
                                                                <Tooltip.Content
                                                                    side="top"
                                                                >
                                                                    Embedding
                                                                    capability
                                                                </Tooltip.Content>
                                                            </Tooltip.Root>
                                                        {/if}
                                                    {:else}
                                                        <!-- Show icons without tooltips when dropdown is closed -->
                                                        {#if model.capabilities.reasoning}
                                                            <Brain
                                                                class="h-3 w-3 text-accent-amber drop-shadow-[0_0_4px_rgba(251,191,36,0.5)]"
                                                            />
                                                        {/if}
                                                        {#if model.capabilities.vision}
                                                            <Eye
                                                                class="h-3 w-3 text-accent-cyan drop-shadow-[0_0_4px_rgba(34,211,238,0.5)]"
                                                            />
                                                        {/if}
                                                        {#if model.capabilities.audio}
                                                            <Headphones
                                                                class="h-3 w-3 text-primary drop-shadow-[0_0_4px_rgba(82,183,136,0.5)]"
                                                            />
                                                        {/if}
                                                        {#if model.capabilities.embedding}
                                                            <Database
                                                                class="h-3 w-3 text-accent-purple drop-shadow-[0_0_4px_rgba(168,85,247,0.5)]"
                                                            />
                                                        {/if}
                                                    {/if}
                                                </div>
                                            {/if}
                                        </div>
                                    </div>
                                </Select.Item>
                            {/each}
                        {/each}
                    {/if}
                </Select.Viewport>
                <Select.ScrollDownButton />
            </Select.Content>
        </Select.Portal>
    </Select.Root>
    </div>

    <!-- Right section: Token counter and utility buttons -->
    <div class="flex items-center gap-1">
        <!-- Token usage counter - always visible -->
        <TokenCounter
            {conversationId}
            modelId={selectedModel}
            {currentMessage}
            {messages}
            {isLoading}
            systemPrompt={selectedSystemPrompt?.content ?? ''}
        />

        <Tooltip.Root>
            <Tooltip.Trigger asChild>
                {#snippet child({ props })}
                    <Button
                        {...props}
                        variant="ghost"
                        size="icon"
                        onclick={removeMessages}
                        class="shrink-0"
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <path d="M5 12h14" />
                            <path d="M12 5v14" />
                        </svg>
                        <span class="sr-only">New Conversation</span>
                    </Button>
                {/snippet}
            </Tooltip.Trigger>
            <Tooltip.Content side="top">Start New Conversation</Tooltip.Content>
        </Tooltip.Root>
    </div>
</div>
