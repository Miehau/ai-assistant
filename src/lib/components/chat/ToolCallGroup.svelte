<script lang="ts">
  import type { ToolCallRecord } from "$lib/types";
  import ToolCallBubble from "./ToolCallBubble.svelte";

  let { calls }: { calls: ToolCallRecord[] } = $props();

  // Auto-expand small groups; collapse large ones by default
  let open = $state(calls.length <= 3);

  let succeeded = $derived(calls.filter((c) => c.success === true).length);
  let failed = $derived(calls.filter((c) => c.success === false && !c.error?.toLowerCase().includes('denied')).length);
  let denied = $derived(calls.filter((c) => c.success === false && c.error?.toLowerCase().includes('denied')).length);
  let running = $derived(calls.filter((c) => c.success === undefined).length);

  let summary = $derived(() => {
    const parts: string[] = [];
    if (succeeded > 0) parts.push(`${succeeded} ✓`);
    if (failed > 0) parts.push(`${failed} ✗`);
    if (denied > 0) parts.push(`${denied} denied`);
    if (running > 0) parts.push(`${running} running`);
    return parts.join('  ');
  });

  function callDotClass(call: ToolCallRecord): string {
    const isDenied = call.success === false && call.error?.toLowerCase().includes('denied');
    if (isDenied) return 'bg-amber-400';
    if (call.success === true) return 'bg-emerald-400';
    if (call.success === false) return 'bg-red-400';
    return 'bg-sky-400 animate-pulse';
  }

  function callNameClass(call: ToolCallRecord): string {
    const isDenied = call.success === false && call.error?.toLowerCase().includes('denied');
    if (isDenied) return 'text-amber-300/80';
    if (call.success === true) return 'text-emerald-300/80';
    if (call.success === false) return 'text-red-300/80';
    return 'text-sky-300/80';
  }

  let hasFailure = $derived(failed > 0 || denied > 0);
  let allSucceeded = $derived(succeeded === calls.length);

  let headerClass = $derived(
    hasFailure
      ? "border-red-500/20 bg-red-500/8 hover:bg-red-500/12"
      : allSucceeded
        ? "border-emerald-500/15 bg-emerald-500/8 hover:bg-emerald-500/12"
        : running > 0
          ? "border-sky-500/20 bg-sky-500/8 hover:bg-sky-500/12"
          : "border-border/40 bg-background/40 hover:bg-background/60"
  );
</script>

{#if calls.length === 1}
  <ToolCallBubble call={calls[0]} />
{:else}
  <div class="w-full min-w-0 rounded-xl border border-border/30 overflow-hidden">
    <!-- Group header -->
    <button
      class={`w-full flex items-center gap-2 px-3 py-2 text-left transition-colors border-b border-border/20 ${headerClass}`}
      onclick={() => (open = !open)}
    >
      <span class="text-[10px] text-muted-foreground/60 shrink-0">{open ? '▾' : '▸'}</span>
      <span class="text-xs font-medium text-foreground/80">{calls.length} tool calls</span>
      <span class="text-[10px] text-muted-foreground/60 ml-auto">{summary()}</span>
    </button>

    {#if open}
      <div class="flex flex-col gap-px p-1.5 bg-background/20">
        {#each calls as call (call.execution_id)}
          <ToolCallBubble {call} />
        {/each}
      </div>
    {:else}
      <!-- Collapsed: colored dot + name per call -->
      <div class="flex flex-wrap items-center gap-x-2 gap-y-1 px-3 py-1.5">
        {#each calls as call (call.execution_id)}
          <span class="inline-flex items-center gap-1 text-[10px] font-mono">
            <span class={`w-1.5 h-1.5 rounded-full shrink-0 ${callDotClass(call)}`}></span>
            <span class={callNameClass(call)}>{call.tool_name}</span>
          </span>
        {/each}
      </div>
    {/if}
  </div>
{/if}
