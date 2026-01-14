<script lang="ts">
  import type { AgentPlan, AgentPlanStep, PhaseKind } from "$lib/types/agent";
  import { getPhaseLabel } from "$lib/types/agent";

  export let phase: PhaseKind | null = null;
  export let plan: AgentPlan | null = null;
  export let steps: AgentPlanStep[] = [];

  const statusStyles: Record<string, string> = {
    Pending: "bg-muted/40 text-muted-foreground",
    Proposed: "bg-amber-500/15 text-amber-300",
    Approved: "bg-emerald-500/15 text-emerald-300",
    Executing: "bg-sky-500/15 text-sky-300",
    Completed: "bg-emerald-500/15 text-emerald-300",
    Failed: "bg-red-500/15 text-red-300",
    Skipped: "bg-zinc-500/15 text-zinc-300",
  };
</script>

<div class="rounded-2xl px-4 py-3 w-full max-w-5xl min-w-0 bg-background/60 border border-border/60">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div>
      <p class="text-[11px] uppercase tracking-wide text-muted-foreground">Agent phase</p>
      <p class="text-sm font-semibold text-foreground">{getPhaseLabel(phase)}</p>
    </div>
    {#if plan?.goal}
      <div class="text-right">
        <p class="text-[11px] uppercase tracking-wide text-muted-foreground">Plan goal</p>
        <p class="text-sm text-foreground">{plan.goal}</p>
      </div>
    {/if}
  </div>

  {#if steps.length > 0}
    <div class="mt-3 space-y-2">
      {#each steps as step (step.id)}
        <div class="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-border/40 bg-background/40 px-3 py-2">
          <div class="min-w-0">
            <p class="text-xs font-medium text-foreground truncate">{step.description}</p>
            <p class="text-[11px] text-muted-foreground truncate">{step.expected_outcome}</p>
          </div>
          <span
            class={`text-[10px] uppercase tracking-wide rounded-full px-2 py-1 ${
              statusStyles[step.status] || "bg-muted/40 text-muted-foreground"
            }`}
          >
            {step.status}
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>
