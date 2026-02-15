<script lang="ts">
  import type { AgentTraceEntry } from "$lib/types/agent";

  export let entries: AgentTraceEntry[] = [];
  export let loading = false;
  export let error: string | null = null;

  const formatTime = (timestamp: number) =>
    new Date(timestamp).toLocaleTimeString();
</script>

<div class="mt-2 rounded-xl border border-border/60 bg-background/40 p-3 text-xs">
  {#if loading}
    <div class="text-muted-foreground">Loading trace...</div>
  {:else if error}
    <div class="text-destructive">{error}</div>
  {:else if entries.length === 0}
    <div class="text-muted-foreground">No trace entries.</div>
  {:else}
    <div class="space-y-3">
      {#each entries as entry}
        <div class="rounded-lg border border-border/40 bg-background/60 p-2">
          <div class="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
            <span class="font-semibold text-foreground/80">{entry.stage}</span>
            <span>iter {entry.iteration_number}</span>
            <span>{formatTime(entry.timestamp_ms)}</span>
          </div>
          <pre class="mt-2 whitespace-pre-wrap break-words rounded-md bg-black/30 p-2 text-[11px] text-foreground/90">
{entry.content}
          </pre>
          {#if entry.metadata}
            <pre class="mt-2 whitespace-pre-wrap break-words rounded-md bg-black/20 p-2 text-[11px] text-foreground/80">
{JSON.stringify(entry.metadata, null, 2)}
            </pre>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
