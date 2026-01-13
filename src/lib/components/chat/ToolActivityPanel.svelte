<script lang="ts">
  import type { ToolActivityEntry } from "$lib/stores/chat";

  export let activities: ToolActivityEntry[] = [];
  export let containerClass = "";

  function formatDuration(duration?: number): string {
    if (!duration || duration <= 0) return "";
    if (duration < 1000) return `${duration} ms`;
    return `${(duration / 1000).toFixed(1)} s`;
  }

  function statusLabel(status: ToolActivityEntry["status"]): string {
    if (status === "running") return "Running";
    if (status === "failed") return "Failed";
    return "Completed";
  }

  function statusClass(status: ToolActivityEntry["status"]): string {
    if (status === "running") return "bg-primary/20 text-primary";
    if (status === "failed") return "bg-red-500/15 text-red-400";
    return "bg-emerald-500/15 text-emerald-400";
  }
</script>

{#if activities.length > 0}
  <div class={`mb-4 w-full rounded-2xl border border-border/60 bg-background/60 backdrop-blur p-4 shadow-sm ${containerClass}`}>
    <div class="flex items-center justify-between">
      <h3 class="text-sm font-semibold text-foreground">Tool activity</h3>
      <span class="text-xs text-muted-foreground">{activities.length} recent</span>
    </div>

    <div class="mt-3 space-y-2">
      {#each activities as activity (activity.execution_id)}
        <div class="flex items-center justify-between gap-3 rounded-xl border border-border/40 bg-background/40 px-3 py-2">
          <div class="min-w-0">
            <p class="text-sm font-medium text-foreground">{activity.tool_name}</p>
            <p class="text-xs text-muted-foreground">
              {statusLabel(activity.status)}
              {#if formatDuration(activity.duration_ms)}
                · {formatDuration(activity.duration_ms)}
              {/if}
            </p>
            {#if activity.status === "failed" && activity.error}
              <p class="text-xs text-red-400 mt-1 truncate">{activity.error}</p>
            {/if}
          </div>
          <span class={`text-[10px] uppercase tracking-wide rounded-full px-2 py-1 ${statusClass(activity.status)}`}>
            {statusLabel(activity.status)}
          </span>
        </div>
      {/each}
    </div>
  </div>
{/if}
