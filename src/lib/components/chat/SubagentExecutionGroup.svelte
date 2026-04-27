<script lang="ts">
  import type { ToolCallRecord } from "$lib/types";
  import type { ToolCallGroup as ToolCallGroupModel } from "$lib/utils/toolCallGrouping";
  import StreamingMarkdown from "../StreamingMarkdown.svelte";
  import SubagentExecutionGroup from "./SubagentExecutionGroup.svelte";
  import ToolCallGroupView from "./ToolCallGroup.svelte";

  let {
    calls,
    sessionId,
    spawnCall,
    childGroups = [],
    depth = 1,
  }: {
    calls: ToolCallRecord[];
    sessionId?: string;
    spawnCall?: ToolCallRecord;
    childGroups?: ToolCallGroupModel[];
    depth?: number;
  } = $props();

  const spawnArgs = $derived(
    spawnCall?.args && typeof spawnCall.args === 'object'
      ? spawnCall.args as Record<string, unknown>
      : {}
  );

  const agentName = $derived.by(() => {
    const rawName = spawnArgs.agent ?? spawnArgs.name ?? spawnArgs.agent_name;
    if (typeof rawName === 'string' && rawName.trim()) return rawName.trim();
    return spawnCall?.tool_name === 'delegate' ? 'default' : undefined;
  });

  /** The task prompt passed to the subagent. */
  const taskPrompt = $derived(
    typeof spawnArgs.task === 'string'
      ? spawnArgs.task
      : typeof spawnArgs.prompt === 'string'
        ? spawnArgs.prompt
        : typeof spawnArgs.message === 'string'
          ? spawnArgs.message
          : undefined
  );

  const title = $derived(
    agentName
      ? `Subagent: ${agentName}`
      : "Subagent"
  );

  const childGroupsBySpawnExecutionId = $derived.by(() => {
    const byExecutionId = new Map<string, ToolCallGroupModel[]>();
    for (const group of childGroups) {
      const executionId = group.spawnCall?.execution_id;
      if (!executionId) continue;
      const existing = byExecutionId.get(executionId) ?? [];
      existing.push(group);
      byExecutionId.set(executionId, existing);
    }
    return byExecutionId;
  });

  const orphanChildGroups = $derived(
    childGroups.filter((group) => !group.spawnCall?.execution_id)
  );

  /** Final text response from the subagent, available after the delegate completes. */
  const responseText = $derived.by(() => {
    if (typeof spawnCall?.result === 'string') return spawnCall.result;
    return spawnCall?.result && typeof spawnCall.result === 'object' && 'response' in spawnCall.result
      ? String((spawnCall.result as Record<string, unknown>).response)
      : undefined
  });

  const responseSuccess = $derived(
    spawnCall?.result && typeof spawnCall.result === 'object' && 'success' in spawnCall.result
      ? Boolean((spawnCall.result as Record<string, unknown>).success)
      : spawnCall?.success
  );

  let isOpen = $state(true);
  let responseOpen = $state(false);

  async function handleMarkdownInteraction(event: MouseEvent | KeyboardEvent) {
    if (
      event instanceof KeyboardEvent &&
      event.key !== "Enter" &&
      event.key !== " "
    ) {
      return;
    }

    const target = event.target as HTMLElement;
    const copyButton = target.closest(".copy-button");
    const code = copyButton?.getAttribute("data-copy");
    if (!code) return;

    event.preventDefault();
    try {
      await navigator.clipboard.writeText(decodeURIComponent(code));
    } catch (error) {
      console.error("Failed to copy:", error);
    }
  }

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
          <span class="text-xs font-semibold text-purple-300">{title}</span>
          {#if depth > 1}
            <span class="text-[9px] text-purple-300/70 bg-purple-500/10 px-1.5 py-0.5 rounded">
              level {depth}
            </span>
          {/if}
          {#if childGroups.length > 0}
            <span class="text-[9px] text-purple-300/70 bg-purple-500/10 px-1.5 py-0.5 rounded">
              {childGroups.length} delegated
            </span>
          {/if}
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
      <div class="pl-4 border-l-2 border-purple-500/20">
        {#if childGroups.length > 0}
          <div class="space-y-2">
            {#each calls as call (call.execution_id)}
              <ToolCallGroupView calls={[call]} />
              {@const nestedGroups = childGroupsBySpawnExecutionId.get(call.execution_id) ?? []}
              {#if nestedGroups.length > 0}
                <div class="ml-3 pl-3 border-l border-purple-400/25 space-y-2">
                  {#each nestedGroups as childGroup (childGroup.sessionId)}
                    <SubagentExecutionGroup
                      calls={childGroup.calls}
                      sessionId={childGroup.sessionId}
                      spawnCall={childGroup.spawnCall}
                      childGroups={childGroup.childGroups ?? []}
                      depth={depth + 1}
                    />
                  {/each}
                </div>
              {/if}
            {/each}
            {#if orphanChildGroups.length > 0}
              <div class="ml-3 pl-3 border-l border-purple-400/25 space-y-2">
                {#each orphanChildGroups as childGroup (childGroup.sessionId)}
                  <SubagentExecutionGroup
                    calls={childGroup.calls}
                    sessionId={childGroup.sessionId}
                    spawnCall={childGroup.spawnCall}
                    childGroups={childGroup.childGroups ?? []}
                    depth={depth + 1}
                  />
                {/each}
              </div>
            {/if}
          </div>
        {:else}
          <ToolCallGroupView {calls} />
        {/if}
      </div>
      {#if responseText}
        <div class="mt-3 pt-3 border-t border-purple-500/20">
          <button
            class="w-full flex items-center gap-2 rounded-lg px-2 py-1.5 text-left text-purple-300/85 hover:bg-purple-500/8 transition-colors"
            onclick={() => (responseOpen = !responseOpen)}
          >
            <span class="text-[10px] text-purple-400/70 shrink-0">{responseOpen ? '▾' : '▸'}</span>
            <span class="text-[10px] uppercase tracking-wide">
              {responseSuccess === false ? "Error" : "Response"}
            </span>
            <span class="ml-auto text-[10px] text-muted-foreground">
              {responseOpen ? "Hide" : "Show"}
            </span>
          </button>
          {#if responseOpen}
            <div class="mt-2 rounded-lg bg-background/50 px-3 py-2.5 text-sm text-foreground break-words">
              <div
                class="prose prose-sm dark:prose-invert max-w-none markdown-content subagent-response-markdown"
                onclick={handleMarkdownInteraction}
                onkeydown={handleMarkdownInteraction}
                role="textbox"
                tabindex="0"
              >
                <StreamingMarkdown content={responseText} />
              </div>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
