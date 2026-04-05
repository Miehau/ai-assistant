<script lang="ts">
  import type { ToolCallRecord } from "$lib/types";
  import ToolCallGroup from "./ToolCallGroup.svelte";

  let { call }: { call: ToolCallRecord } = $props();

  /** The workflow name from the tool call args. */
  const workflowName = $derived(
    call.args && typeof call.args === 'object' && 'name' in call.args
      ? String((call.args as Record<string, unknown>).name)
      : 'workflow'
  );

  /**
   * Convert accumulated progress text into synthetic ToolCallRecords.
   *
   * Each step emits two lines: `[name] {"phase":"name","status":"running"}`
   * then `[name] {"phase":"name","status":"done","output":...}`.
   * We deduplicate by step name, keeping the latest (which has output).
   */
  const stepCalls = $derived.by((): ToolCallRecord[] => {
    const text = call.workflowProgress ?? '';
    if (!text) return [];
    const lines = text.split('\n').filter((line) => line.trim().length > 0);

    // Parse all lines, deduplicating by step name (later lines override earlier)
    const stepMap = new Map<string, { index: number; stepName: string; args: Record<string, unknown>; result: unknown; status: string; durationMs?: number }>();
    let orderIndex = 0;

    for (const line of lines) {
      const match = line.match(/^\[([^\]]+)\]\s*(.*)$/);
      const event = match ? match[1] : 'info';
      const data = match ? match[2] : line;

      let parsed: unknown;
      try { parsed = JSON.parse(data); } catch { parsed = data; }

      const parsedObj = typeof parsed === 'object' && parsed !== null ? parsed as Record<string, unknown> : null;
      const stepName = parsedObj
        ? String(parsedObj.phase ?? parsedObj.name ?? parsedObj.action ?? event)
        : event;

      const status = parsedObj?.status as string ?? 'running';
      const output = parsedObj?.output;
      const durationMs = parsedObj?.durationMs as number | undefined;

      // Strip metadata fields from args
      let args: Record<string, unknown>;
      if (parsedObj) {
        const { phase: _p, name: _n, status: _s, output: _o, durationMs: _d, error: _e, ...rest } = parsedObj as Record<string, unknown>;
        args = Object.keys(rest).length > 0 ? rest : {};
      } else {
        args = typeof parsed === 'string' && parsed ? { value: parsed } : {};
      }

      const existing = stepMap.get(stepName);
      if (existing) {
        // Update with newer data (completion overrides start)
        existing.status = status;
        existing.durationMs = durationMs;
        if (output !== undefined) existing.result = output;
        if (Object.keys(args).length > 0) existing.args = args;
      } else {
        stepMap.set(stepName, { index: orderIndex++, stepName, args, result: output, status, durationMs });
      }
    }

    // Convert to ToolCallRecords, sorted by insertion order
    return [...stepMap.values()]
      .sort((a, b) => a.index - b.index)
      .map((entry) => {
        const resultStr = entry.result !== undefined
          ? (typeof entry.result === 'string' ? entry.result : JSON.stringify(entry.result))
          : undefined;

        return {
          execution_id: `${call.execution_id}__step_${entry.index}`,
          tool_name: entry.stepName,
          args: entry.args,
          result: resultStr,
          duration_ms: entry.durationMs,
          success: entry.status === 'done' ? true
            : entry.status === 'failed' ? false
            : undefined,
        } satisfies ToolCallRecord;
      });
  });

  let isOpen = $state(true);

  const isRunning = $derived(call.success === undefined);
  const isFailed = $derived(call.success === false);

  const completedCount = $derived(stepCalls.filter((c) => c.success === true).length);
  const runningCount = $derived(stepCalls.filter((c) => c.success === undefined).length);

  const statusLabel = $derived(
    isRunning ? 'running' : isFailed ? 'failed' : 'completed'
  );

  const totalDuration = $derived(call.duration_ms ?? 0);

  function formatResult(value: unknown): string {
    if (value === undefined || value === null) return '';
    if (typeof value === 'string') {
      // Try to parse as JSON for pretty-printing
      try {
        return JSON.stringify(JSON.parse(value), null, 2);
      } catch {
        return value;
      }
    }
    return JSON.stringify(value, null, 2);
  }

  function formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  const borderClass = $derived(
    isRunning
      ? 'border-amber-500/30'
      : isFailed
        ? 'border-amber-500/40'
        : 'border-amber-500/25'
  );

  const bgClass = $derived(
    isRunning
      ? 'bg-amber-500/10'
      : isFailed
        ? 'bg-amber-500/15'
        : 'bg-amber-500/8'
  );
</script>

<div class={`rounded-2xl border ${borderClass} ${bgClass} overflow-hidden w-full max-w-5xl min-w-0`}>
  <button
    onclick={() => (isOpen = !isOpen)}
    class="w-full px-4 py-3 flex items-center justify-between hover:bg-amber-500/5 transition-colors"
  >
    <div class="flex items-center gap-3 min-w-0">
      <svg
        class="w-4 h-4 text-amber-400 transition-transform {isOpen ? 'rotate-90' : ''}"
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
          <span class="text-xs font-semibold text-amber-300">Workflow</span>
          <span class="text-[10px] text-amber-400/70 font-mono">{workflowName}</span>
        </div>
        <p class="text-[10px] text-muted-foreground">
          {#if stepCalls.length > 0}
            {stepCalls.length} step{stepCalls.length === 1 ? '' : 's'} ·
          {/if}
          {statusLabel}
          {#if totalDuration > 0}
            · {formatDuration(totalDuration)}
          {/if}
        </p>
      </div>
    </div>
    <div class="flex items-center gap-2">
      {#if completedCount > 0}
        <span class="text-[9px] bg-emerald-500/20 text-emerald-300 px-1.5 py-0.5 rounded">
          {completedCount}
        </span>
      {/if}
      {#if isFailed}
        <span class="text-[9px] bg-red-500/20 text-red-300 px-1.5 py-0.5 rounded">
          failed
        </span>
      {/if}
      {#if runningCount > 0}
        <span class="text-[9px] bg-amber-500/20 text-amber-300 px-1.5 py-0.5 rounded animate-pulse">
          {runningCount}
        </span>
      {/if}
    </div>
  </button>

  {#if isOpen}
    <div class="px-3 pb-3 space-y-2">
      {#if stepCalls.length > 0 || isRunning}
        <div class="pl-4 border-l-2 border-amber-500/20">
          {#if stepCalls.length > 0}
            <ToolCallGroup calls={stepCalls} />
          {:else}
            <div class="flex items-center gap-2 py-2 px-2 text-[11px] text-amber-400/60">
              <span class="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse shrink-0"></span>
              Starting workflow...
            </div>
          {/if}
        </div>
      {/if}
      {#if !isRunning && call.success !== undefined}
        <div class={stepCalls.length > 0 ? 'mt-3 pt-3 border-t border-amber-500/20' : ''}>
          <details open={stepCalls.length === 0}>
            <summary class="text-[10px] uppercase tracking-wide text-amber-400/60 cursor-pointer mb-1.5 px-1">
              {isFailed ? 'Error' : 'Result'}
            </summary>
            <div class="rounded-lg bg-background/50 px-3 py-2.5 text-sm text-foreground whitespace-pre-wrap break-words max-h-48 overflow-auto">
              <pre class="text-[11px] font-mono">{isFailed ? (call.error ?? 'Unknown error') : formatResult(call.result)}</pre>
            </div>
          </details>
        </div>
      {/if}
    </div>
  {/if}
</div>
