<script lang="ts">
  import type { ToolCallRecord } from "$lib/types";

  export let call: ToolCallRecord;

  function formatToolPayload(payload: unknown): string {
    if (payload === undefined) return "";
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

  function toolStatusLabel(): string {
    if (call.success === true) return "executed";
    if (call.success === false) return "failed";
    return "running";
  }

  function toolBubbleClass(): string {
    if (call.success === true) {
      return "border-emerald-500/20 bg-emerald-500/10 hover:bg-emerald-500/15";
    }
    if (call.success === false) {
      return "border-red-500/25 bg-red-500/10 hover:bg-red-500/15";
    }
    return "border-sky-500/25 bg-sky-500/10 hover:bg-sky-500/15";
  }

  function toolPillClass(): string {
    if (call.success === true) return "bg-emerald-500/20 text-emerald-200";
    if (call.success === false) return "bg-red-500/20 text-red-200";
    return "bg-sky-500/20 text-sky-200";
  }
</script>

<div class={`rounded-2xl px-4 py-2 w-full max-w-5xl min-w-0 border ${toolBubbleClass()}`}>
  <details class="group">
    <summary class="list-none cursor-pointer">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="min-w-0">
          <p class="text-xs font-semibold text-foreground">{call.tool_name}</p>
          <p class="text-[10px] text-muted-foreground">
            {toolStatusLabel()}
            {#if formatToolDuration(call.duration_ms)}
              · {formatToolDuration(call.duration_ms)}
            {/if}
          </p>
        </div>
        <span class={`text-[10px] uppercase tracking-wide rounded-full px-2 py-1 ${toolPillClass()}`}>
          {toolStatusLabel()}
        </span>
      </div>
    </summary>

    <div class="mt-3 grid gap-2 md:grid-cols-2">
      <div>
        <p class="text-[10px] uppercase tracking-wide text-muted-foreground mb-1">Input</p>
        <pre class="max-h-40 overflow-auto rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.args)}
        </pre>
      </div>
      <div>
        <p class="text-[10px] uppercase tracking-wide text-muted-foreground mb-1">
          {call.success === false ? "Error" : "Output"}
        </p>
        <pre class="max-h-40 overflow-auto rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.success === false ? call.error : call.result)}
        </pre>
      </div>
    </div>
  </details>
</div>
