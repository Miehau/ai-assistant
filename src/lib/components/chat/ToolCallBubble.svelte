<script lang="ts">
  import type { ToolCallRecord } from "$lib/types";

  let { call }: { call: ToolCallRecord } = $props();

  /** Strip noisy fields (e.g. HTTP headers) and pretty-print JSON */
  function formatToolPayload(payload: unknown): string {
    if (payload === undefined || payload === null) return "";
    if (typeof payload === 'string') {
      // Try to parse JSON strings for pretty-printing
      try {
        const parsed = JSON.parse(payload);
        return JSON.stringify(stripNoise(parsed), null, 2);
      } catch {
        return payload;
      }
    }
    try {
      return JSON.stringify(stripNoise(payload), null, 2);
    } catch {
      return String(payload);
    }
  }

  /** Remove noisy fields like HTTP headers from API responses */
  function stripNoise(val: unknown): unknown {
    if (typeof val !== 'object' || val === null || Array.isArray(val)) return val;
    const obj = val as Record<string, unknown>;
    if ('headers' in obj) {
      const { headers: _, ...rest } = obj;
      return rest;
    }
    return obj;
  }

  let copiedField: 'input' | 'output' | null = $state(null);

  async function copyPayload(field: 'input' | 'output') {
    const text = field === 'input'
      ? formatToolPayload(call.args)
      : formatToolPayload(call.success === false ? call.error : call.result);
    if (!text) return;
    await navigator.clipboard.writeText(text);
    copiedField = field;
    setTimeout(() => { copiedField = null; }, 1500);
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
        <div class="flex items-center justify-between mb-1">
          <p class="text-[10px] uppercase tracking-wide text-muted-foreground">Input</p>
          {#if formatToolPayload(call.args)}
            <button
              onclick={() => copyPayload('input')}
              class="text-[9px] text-muted-foreground/50 hover:text-muted-foreground transition-colors px-1"
            >{copiedField === 'input' ? 'copied' : 'copy'}</button>
          {/if}
        </div>
        <pre class="max-h-40 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.args)}
        </pre>
      </div>
      <div class="min-w-0">
        <div class="flex items-center justify-between mb-1">
          <p class="text-[10px] uppercase tracking-wide text-muted-foreground">
            {call.success === false ? "Error" : "Output"}
          </p>
          {#if formatToolPayload(call.success === false ? call.error : call.result)}
            <button
              onclick={() => copyPayload('output')}
              class="text-[9px] text-muted-foreground/50 hover:text-muted-foreground transition-colors px-1"
            >{copiedField === 'output' ? 'copied' : 'copy'}</button>
          {/if}
        </div>
        <pre class="max-h-40 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.success === false ? call.error : call.result)}
        </pre>
      </div>
    </div>
  </details>
</div>
