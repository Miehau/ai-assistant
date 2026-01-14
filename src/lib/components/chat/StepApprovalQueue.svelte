<script lang="ts">
  import type { AgentStepProposedPayload } from "$lib/types/events";
  import type { AgentPlanStep } from "$lib/types/agent";
  import { resolveStepApproval } from "$lib/stores/chat";

  export let approvals: AgentStepProposedPayload[] = [];
  export let containerClass = "";

  let feedbackById: Record<string, string> = {};

  function updateFeedback(approvalId: string, value: string) {
    feedbackById = { ...feedbackById, [approvalId]: value };
  }

  function sendDecision(approvalId: string, decision: 'approved' | 'skipped' | 'modified' | 'denied') {
    resolveStepApproval(approvalId, decision, feedbackById[approvalId]);
  }
</script>

<div class={containerClass}>
  <div class="rounded-2xl px-4 py-3 bg-background/60 border border-border/60 space-y-3">
    <div class="flex items-center justify-between">
      <div>
        <p class="text-[11px] uppercase tracking-wide text-muted-foreground">Step approvals</p>
        <p class="text-sm font-semibold text-foreground">Awaiting your input</p>
      </div>
      <span class="text-[10px] uppercase tracking-wide rounded-full px-2 py-1 bg-amber-500/15 text-amber-300">
        Approval
      </span>
    </div>

    {#each approvals as approval (approval.approval_id)}
      <div class="rounded-xl border border-border/40 bg-background/40 px-3 py-3 space-y-2">
        <div>
          <p class="text-xs text-muted-foreground uppercase tracking-wide">Risk</p>
          <p class="text-sm text-foreground">{approval.risk}</p>
        </div>
        <div>
          <p class="text-xs text-muted-foreground uppercase tracking-wide">Step</p>
          <p class="text-sm text-foreground">
            {(approval.step as AgentPlanStep)?.description || "Pending step"}
          </p>
        </div>
        {#if approval.preview}
          <pre class="max-h-48 overflow-auto rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">
{JSON.stringify(approval.preview, null, 2)}
          </pre>
        {/if}
        <textarea
          rows="2"
          class="w-full rounded-lg border border-border/60 bg-background/60 px-3 py-2 text-xs text-foreground"
          placeholder="Optional feedback for modify/deny..."
          value={feedbackById[approval.approval_id]}
          on:input={(e) => updateFeedback(approval.approval_id, (e.target as HTMLTextAreaElement).value)}
        ></textarea>
        <div class="flex flex-wrap gap-2">
          <button
            class="rounded-full px-3 py-1 text-xs bg-emerald-500/20 text-emerald-200"
            on:click={() => sendDecision(approval.approval_id, "approved")}
          >
            Approve
          </button>
          <button
            class="rounded-full px-3 py-1 text-xs bg-zinc-500/20 text-zinc-200"
            on:click={() => sendDecision(approval.approval_id, "skipped")}
          >
            Skip
          </button>
          <button
            class="rounded-full px-3 py-1 text-xs bg-amber-500/20 text-amber-200"
            on:click={() => sendDecision(approval.approval_id, "modified")}
          >
            Modify
          </button>
          <button
            class="rounded-full px-3 py-1 text-xs bg-red-500/20 text-red-200"
            on:click={() => sendDecision(approval.approval_id, "denied")}
          >
            Deny
          </button>
        </div>
      </div>
    {/each}
  </div>
</div>
