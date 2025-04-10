<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import * as Tooltip from "$lib/components/ui/tooltip";
  import { Button } from "$lib/components/ui/button";
  import * as Select from "$lib/components/ui/select";
  import type { Model } from "$lib/types/models";
  import type { SystemPrompt } from "$lib/types";
  import type { Selected } from "bits-ui";
  import { Eye, Headphones, Zap, Database, Brain } from "lucide-svelte";

  export let availableModels: Model[] = [];
  export let systemPrompts: SystemPrompt[] = [];
  export let selectedModel: Selected<string>;
  export let selectedSystemPrompt: SystemPrompt | null = null;
  export let streamingEnabled: boolean = true;

  const dispatch = createEventDispatcher();

  function selectModel(v: Selected<string> | undefined) {
    if (v) {
      const model = availableModels.find((m) => m.model_name === v.value);
      selectedModel = {
        value: v.value, // This is the technical model_name
        label: `${model?.name || v.value} • ${model?.provider ?? ""}`,
      };
      dispatch('modelSelect', selectedModel);
    }
  }

  function selectSystemPrompt(prompt: SystemPrompt) {
    selectedSystemPrompt = prompt;
    dispatch('systemPromptSelect', prompt);
  }

  function toggleStreaming() {
    streamingEnabled = !streamingEnabled;
    dispatch('toggleStreaming', streamingEnabled);
  }

  function removeMessages() {
    dispatch('removeMessages');
  }

  // Group models by provider for better organization in the dropdown
  function getProviderGroups(models: Model[]) {
    const groups: {provider: string, models: Model[]}[] = [];
    
    // Group models by provider
    models.forEach(model => {
      const existingGroup = groups.find(g => g.provider === model.provider);
      if (existingGroup) {
        existingGroup.models.push(model);
      } else {
        groups.push({
          provider: model.provider,
          models: [model]
        });
      }
    });
    
    // Sort groups alphabetically by provider name
    groups.sort((a, b) => a.provider.localeCompare(b.provider));
    
    // Sort models within each group by name
    groups.forEach(group => {
      group.models.sort((a, b) => a.model_name.localeCompare(b.model_name));
    });
    
    return groups;
  }
</script>

<div class="flex flex-wrap items-center gap-2">
  <Tooltip.Root>
    <Tooltip.Trigger asChild let:builder>
      <Button
        builders={[builder]}
        variant="ghost"
        size="icon"
        on:click={removeMessages}
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
    </Tooltip.Trigger>
    <Tooltip.Content side="top">Start New Conversation</Tooltip.Content>
  </Tooltip.Root>

  <Select.Root
    selected={{ value: selectedSystemPrompt?.id ?? "", label: selectedSystemPrompt?.name ?? "" }}
    onSelectedChange={(v) => {
      if (v) {
        const prompt = systemPrompts.find((p) => p.id === v.value);
        if (prompt) selectSystemPrompt(prompt);
      }
    }}
  >
    <Select.Trigger class="min-w-[180px] w-fit mr-2">
      <Select.Value placeholder="Select system prompt">
        {#if selectedSystemPrompt}
          <div class="flex items-center gap-2">
            <span class="truncate max-w-[150px]">{selectedSystemPrompt.name}</span>
          </div>
        {/if}
      </Select.Value>
    </Select.Trigger>
    <Select.Content>
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
    </Select.Content>
  </Select.Root>

  <Select.Root onSelectedChange={selectModel} selected={selectedModel}>
    <Select.Trigger class="min-w-[180px] w-fit justify-between">
      <Select.Value placeholder="Select a model">
        {#if selectedModel}
          <div class="flex items-center gap-2">
            {#each availableModels as model}
              {#if model.model_name === selectedModel.value}
                <span>{model.name || model.model_name}</span>
              {/if}
            {/each}
          </div>
        {/if}
      </Select.Value>
    </Select.Trigger>
    <Select.Content class="w-[350px] text-xs">
      {#if availableModels.length === 0}
        <Select.Item value="" disabled>No models available</Select.Item>
      {:else}
        <!-- Group models by provider -->
        {#each getProviderGroups(availableModels) as group}
          <div class="px-2 py-1.5 text-xs font-semibold text-muted-foreground bg-muted/50">
            {group.provider}
          </div>
          
          {#each group.models as model}
            <Select.Item value={model.model_name} class="py-2">
              <div class="flex flex-col gap-1 w-full">
                <div class="grid grid-cols-[1fr_auto] items-center w-full">
                  <span class="font-medium text-xs">{model.name || model.model_name}</span>
                  
                  {#if model.capabilities}
                    <div class="flex gap-1 justify-self-end">
                      {#if model.capabilities.reasoning}
                        <Tooltip.Root>
                          <Tooltip.Trigger asChild let:builder>
                            <span class="cursor-help">
                              <Brain class="h-3 w-3 text-amber-500" />
                            </span>
                          </Tooltip.Trigger>
                          <Tooltip.Content side="top">
                            Advanced reasoning
                          </Tooltip.Content>
                        </Tooltip.Root>
                      {/if}
                      {#if model.capabilities.vision}
                        <Tooltip.Root>
                          <Tooltip.Trigger asChild let:builder>
                            <span class="cursor-help">
                              <Eye class="h-3 w-3 text-blue-500" />
                            </span>
                          </Tooltip.Trigger>
                          <Tooltip.Content side="top">
                            Vision capability
                          </Tooltip.Content>
                        </Tooltip.Root>
                      {/if}
                      {#if model.capabilities.audio}
                        <Tooltip.Root>
                          <Tooltip.Trigger asChild let:builder>
                            <span class="cursor-help">
                              <Headphones class="h-3 w-3 text-green-500" />
                            </span>
                          </Tooltip.Trigger>
                          <Tooltip.Content side="top">
                            Audio capability
                          </Tooltip.Content>
                        </Tooltip.Root>
                      {/if}
                      {#if model.capabilities.embedding}
                        <Tooltip.Root>
                          <Tooltip.Trigger asChild let:builder>
                            <span class="cursor-help">
                              <Database class="h-3 w-3 text-purple-500" />
                            </span>
                          </Tooltip.Trigger>
                          <Tooltip.Content side="top">
                            Embedding capability
                          </Tooltip.Content>
                        </Tooltip.Root>
                      {/if}
                    </div>
                  {/if}
                </div>
              </div>
            </Select.Item>
          {/each}
        {/each}
      {/if}
    </Select.Content>
  </Select.Root>

  <Tooltip.Root>
    <Tooltip.Trigger asChild let:builder>
      <Button
        builders={[builder]}
        variant="ghost"
        size="icon"
        type="button"
        on:click={toggleStreaming}
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
          class={streamingEnabled ? "text-primary" : "text-muted-foreground"}
        >
          <path d="M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242" />
          <path d="M12 12v9" />
          <path d="m8 17 4 4 4-4" />
        </svg>
        <span class="sr-only">Toggle Streaming</span>
      </Button>
    </Tooltip.Trigger>
    <Tooltip.Content side="top">
      {streamingEnabled ? 'Disable' : 'Enable'} Streaming
    </Tooltip.Content>
  </Tooltip.Root>
</div>
