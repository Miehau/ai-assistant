<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import * as Tooltip from "$lib/components/ui/tooltip";
  import { Button } from "$lib/components/ui/button";
  import * as Select from "$lib/components/ui/select";
  import type { Model } from "$lib/types/models";
  import type { SystemPrompt } from "$lib/types";
  import type { Selected } from "bits-ui";

  export let availableModels: Model[] = [];
  export let systemPrompts: SystemPrompt[] = [];
  export let selectedModel: Selected<string>;
  export let selectedSystemPrompt: SystemPrompt | null = null;
  export let streamingEnabled: boolean = true;

  const dispatch = createEventDispatcher();

  function selectModel(v: Selected<string> | undefined) {
    if (v) {
      selectedModel = {
        value: v.value,
        label: `${v.value} • ${availableModels.find((m) => m.model_name === v.value)?.provider ?? ""}`,
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
    <Select.Trigger class="min-w-[180px] w-fit">
      <Select.Value placeholder="Select a model">
        {#if selectedModel}
          <div class="flex items-center gap-2">
            <span>{selectedModel.value}</span>
            {#if selectedModel.label}
              <span class="text-sm text-muted-foreground">•</span>
              <span class="text-sm text-muted-foreground">
                {selectedModel.label.split(" • ")[1] ?? ""}
              </span>
            {/if}
          </div>
        {/if}
      </Select.Value>
    </Select.Trigger>
    <Select.Content>
      {#if availableModels.length === 0}
        <Select.Item value="" disabled>No models available</Select.Item>
      {:else}
        {#each availableModels as model}
          <Select.Item value={model.model_name}>
            <div class="flex items-center gap-2 whitespace-nowrap">
              <span>{model.model_name}</span>
              <span class="text-sm text-muted-foreground">•</span>
              <span class="text-sm text-muted-foreground"
                >{model.provider}</span
              >
            </div>
          </Select.Item>
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
