<script lang="ts">
  import type { AgentNeedsHumanInputPayload } from "$lib/types/events";
  import { submitHumanInput } from "$lib/stores/chat";

  export let prompt: AgentNeedsHumanInputPayload | null = null;
  export let containerClass = "";

  let inputValue = "";

  function sendInput() {
    if (!prompt || !inputValue.trim()) return;
    submitHumanInput(prompt.request_id, inputValue.trim());
    inputValue = "";
  }
</script>

{#if prompt}
  <div class={containerClass}>
    <div class="rounded-2xl px-4 py-3 bg-background/60 border border-border/60 space-y-3">
      <div>
        <p class="text-[11px] uppercase tracking-wide text-muted-foreground">Agent question</p>
        <p class="text-sm text-foreground">{prompt.question}</p>
      </div>
      <div class="flex flex-wrap gap-2">
        <input
          class="flex-1 min-w-[200px] rounded-lg border border-border/60 bg-background/60 px-3 py-2 text-xs text-foreground"
          placeholder="Type your response..."
          bind:value={inputValue}
        />
        <button
          class="rounded-full px-3 py-1 text-xs bg-emerald-500/20 text-emerald-200"
          on:click={sendInput}
        >
          Send
        </button>
      </div>
    </div>
  </div>
{/if}
