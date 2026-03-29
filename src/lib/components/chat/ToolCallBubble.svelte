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
    if (duration < 1000) return `${duration}ms`;
    return `${(duration / 1000).toFixed(1)}s`;
  }

  let isDenied = $derived(
    call.success === false && call.error?.toLowerCase().includes('denied')
  );

  let statusLabel = $derived(
    isDenied ? "denied" : call.success === true ? "executed" : call.success === false ? "failed" : "running"
  );

  let dotClass = $derived(
    isDenied
      ? "bg-amber-400"
      : call.success === true
        ? "bg-emerald-400"
        : call.success === false
          ? "bg-red-400"
          : "bg-sky-400 animate-pulse"
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
</script>

<div class={`rounded-xl px-3 py-2 w-full min-w-0 border transition-colors ${bubbleClass}`}>
  <details class="group">
    <summary class="list-none cursor-pointer">
      <div class="flex items-center gap-2 min-w-0">
        <span class={`shrink-0 w-1.5 h-1.5 rounded-full ${dotClass}`}></span>
        <span class="text-xs font-mono font-medium text-foreground truncate flex-1 min-w-0">{call.tool_name}</span>
        <span class="text-[10px] text-muted-foreground/70 shrink-0">
          {statusLabel}{formatToolDuration(call.duration_ms) ? ` · ${formatToolDuration(call.duration_ms)}` : ''}
        </span>
        <span class="text-muted-foreground/40 text-[10px] shrink-0 group-open:hidden">▸</span>
        <span class="text-muted-foreground/40 text-[10px] shrink-0 hidden group-open:inline">▾</span>
      </div>
    </summary>

    <div class="mt-2 grid gap-2 md:grid-cols-2">
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
