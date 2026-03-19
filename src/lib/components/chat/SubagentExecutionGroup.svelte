<script lang="ts">
  import type { ToolCallRecord } from "$lib/types";
  import ToolCallBubble from "./ToolCallBubble.svelte";

  let { calls, sessionId, spawnCall }: { calls: ToolCallRecord[]; sessionId?: string; spawnCall?: ToolCallRecord } = $props();

  /** The task prompt passed to the subagent. */
  const taskPrompt = $derived(
    spawnCall?.args && typeof spawnCall.args === 'object' && 'prompt' in spawnCall.args
      ? String((spawnCall.args as Record<string, unknown>).prompt)
      : undefined
  );

  /** Final text response from the subagent, available after agent.spawn completes. */
  const responseText = $derived(
    spawnCall?.result && typeof spawnCall.result === 'object' && 'response' in spawnCall.result
      ? String((spawnCall.result as Record<string, unknown>).response)
      : undefined
  );

  const responseSuccess = $derived(
    spawnCall?.result && typeof spawnCall.result === 'object' && 'success' in spawnCall.result
      ? Boolean((spawnCall.result as Record<string, unknown>).success)
      : undefined
  );

  let isOpen = $state(true);

  const completedCount = $derived(calls.filter((c) => c.success === true).length);
  const failedCount = $derived(calls.filter((c) => c.success === false).length);
  const runningCount = $derived(calls.filter((c) => c.success === undefined).length);

  const statusLabel = $derived(
    runningCount > 0
      ? "running"
      : failedCount > 0
        ? "completed with errors"
        : "completed"
  );

  const totalDuration = $derived(
    calls.reduce((sum, call) => sum + (call.duration_ms || 0), 0)
  );

  function formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  const borderClass = $derived(
    runningCount > 0
      ? "border-purple-500/30"
      : failedCount > 0
        ? "border-purple-500/40"
        : "border-purple-500/25"
  );

  const bgClass = $derived(
    runningCount > 0
      ? "bg-purple-500/10"
      : failedCount > 0
        ? "bg-purple-500/15"
        : "bg-purple-500/8"
  );
</script>

<div class={`rounded-2xl border ${borderClass} ${bgClass} overflow-hidden w-full max-w-5xl min-w-0`}>
  <button
    onclick={() => (isOpen = !isOpen)}
    class="w-full px-4 py-3 flex items-center justify-between hover:bg-purple-500/5 transition-colors"
  >
    <div class="flex items-center gap-3 min-w-0">
      <svg
        class="w-4 h-4 text-purple-400 transition-transform {isOpen ? 'rotate-90' : ''}"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M9 5l7 7-7 7"
        />
      </svg>
      <div class="text-left min-w-0 overflow-hidden">
        <div class="flex items-center gap-2 flex-wrap">
          <span class="text-xs font-semibold text-purple-300">Subagent</span>
          {#if sessionId}
            <span class="text-[9px] text-purple-400/60 font-mono">
              {sessionId.slice(0, 8)}
            </span>
          {/if}
        </div>
        {#if taskPrompt}
          <p class="text-[11px] text-purple-200/75 truncate mt-0.5">{taskPrompt}</p>
        {/if}
        <p class="text-[10px] text-muted-foreground">
          {calls.length} tool{calls.length === 1 ? "" : "s"} · {statusLabel}
          {#if totalDuration > 0}
            · {formatDuration(totalDuration)}
          {/if}
        </p>
      </div>
    </div>
    <div class="flex items-center gap-2">
      {#if completedCount > 0}
        <span
          class="text-[9px] bg-emerald-500/20 text-emerald-300 px-1.5 py-0.5 rounded"
        >
          {completedCount}
        </span>
      {/if}
      {#if failedCount > 0}
        <span class="text-[9px] bg-red-500/20 text-red-300 px-1.5 py-0.5 rounded">
          {failedCount}
        </span>
      {/if}
      {#if runningCount > 0}
        <span class="text-[9px] bg-sky-500/20 text-sky-300 px-1.5 py-0.5 rounded">
          {runningCount}
        </span>
      {/if}
    </div>
  </button>

  {#if isOpen}
    <div class="px-3 pb-3 space-y-2">
      {#each calls as call (call.execution_id)}
        <div class="pl-4 border-l-2 border-purple-500/20">
          <ToolCallBubble {call} />
        </div>
      {/each}
      {#if responseText}
        <div class="mt-3 pt-3 border-t border-purple-500/20">
          <p class="text-[10px] uppercase tracking-wide text-purple-400/60 mb-1.5 px-1">
            {responseSuccess === false ? "Error" : "Response"}
          </p>
          <div class="rounded-lg bg-background/50 px-3 py-2.5 text-sm text-foreground whitespace-pre-wrap break-words">
            {responseText}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>
