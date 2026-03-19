<script lang="ts">
  import type { ToolCallRecord } from "$lib/types";

  let { call }: { call: ToolCallRecord } = $props();

  function formatToolPayload(payload: unknown): string {
    if (payload === undefined || payload === null) return "";
    if (typeof payload === 'string') return payload;
    try {
      return JSON.stringify(payload, null, 2);
    } catch {
      return String(payload);
    }
  }

  function formatToolDuration(duration?: number): string {
    if (duration === undefined || duration === null) return "";
    if (duration < 1000) return `${duration} ms`;
    return `${(duration / 1000).toFixed(1)} s`;
  }

  let isDenied = $derived(
    call.success === false && call.error?.toLowerCase().includes('denied')
  );

  let statusLabel = $derived(
    isDenied ? "denied" : call.success === true ? "executed" : call.success === false ? "failed" : "running"
  );

  let bubbleClass = $derived(
    isDenied
      ? "border-amber-500/25 bg-amber-500/10 hover:bg-amber-500/15"
      : call.success === true
        ? "border-emerald-500/20 bg-emerald-500/10 hover:bg-emerald-500/15"
        : call.success === false
          ? "border-red-500/25 bg-red-500/10 hover:bg-red-500/15"
          : "border-sky-500/25 bg-sky-500/10 hover:bg-sky-500/15"
  );

  let pillClass = $derived(
    isDenied
      ? "bg-amber-500/20 text-amber-200"
      : call.success === true
        ? "bg-emerald-500/20 text-emerald-200"
        : call.success === false
          ? "bg-red-500/20 text-red-200"
          : "bg-sky-500/20 text-sky-200"
  );
</script>

<div class={`rounded-2xl px-4 py-2 w-full max-w-5xl min-w-0 border ${bubbleClass}`}>
  <details class="group">
    <summary class="list-none cursor-pointer">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="min-w-0">
          <p class="text-xs font-semibold text-foreground">{call.tool_name}</p>
          <p class="text-[10px] text-muted-foreground">
            {statusLabel}
            {#if formatToolDuration(call.duration_ms)}
              · {formatToolDuration(call.duration_ms)}
            {/if}
          </p>
        </div>
        <span class={`text-[10px] uppercase tracking-wide rounded-full px-2 py-1 ${pillClass}`}>
          {statusLabel}
        </span>
      </div>
    </summary>

    <div class="mt-3 grid gap-2 md:grid-cols-2">
      <div class="min-w-0">
        <p class="text-[10px] uppercase tracking-wide text-muted-foreground mb-1">Input</p>
        <pre class="max-h-40 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.args)}
        </pre>
      </div>
      <div class="min-w-0">
        <p class="text-[10px] uppercase tracking-wide text-muted-foreground mb-1">
          {call.success === false ? "Error" : "Output"}
        </p>
        <pre class="max-h-40 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.success === false ? call.error : call.result)}
        </pre>
      </div>
    </div>
  </details>
</div>
