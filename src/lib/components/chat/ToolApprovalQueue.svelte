<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import type { ToolExecutionProposedPayload } from "$lib/types/events";
  import { resolveToolApproval } from "$lib/stores/chat";

  export let approvals: ToolExecutionProposedPayload[] = [];

  function formatPreview(preview: ToolExecutionProposedPayload["preview"]): string {
    if (!preview) return "";
    if (typeof preview === "string") return preview;
    if (typeof preview === "object" && preview && "diff" in preview) {
      const diff = (preview as { diff?: unknown }).diff;
      if (typeof diff === "string") {
        return diff;
      }
    }
    try {
      return JSON.stringify(preview, null, 2);
    } catch {
      return String(preview);
    }
  }

  function previewTitle(preview: ToolExecutionProposedPayload["preview"]): string | null {
    if (!preview || typeof preview !== "object") return null;
    if ("path" in preview && typeof (preview as { path?: unknown }).path === "string") {
      return (preview as { path: string }).path;
    }
    return null;
  }
</script>

{#if approvals.length > 0}
  <div class="mb-4 rounded-2xl border border-border/60 bg-background/60 backdrop-blur p-4 shadow-sm">
    <div class="flex items-center justify-between">
      <h3 class="text-sm font-semibold text-foreground">Tool approvals required</h3>
      <span class="text-xs text-muted-foreground">{approvals.length} pending</span>
    </div>

    <div class="mt-3 space-y-4">
      {#each approvals as approval (approval.approval_id)}
        <div class="rounded-xl border border-border/40 bg-background/40 p-3">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div>
              <p class="text-sm font-medium text-foreground">{approval.tool_name}</p>
              {#if previewTitle(approval.preview)}
                <p class="text-xs text-muted-foreground">
                  {previewTitle(approval.preview)}
                </p>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              <Button
                size="sm"
                on:click={() => resolveToolApproval(approval.approval_id, true)}
              >
                Approve
              </Button>
              <Button
                variant="outline"
                size="sm"
                on:click={() => resolveToolApproval(approval.approval_id, false)}
              >
                Deny
              </Button>
            </div>
          </div>

          {#if approval.preview}
            <pre class="mt-3 max-h-64 overflow-auto rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">
{formatPreview(approval.preview)}
            </pre>
          {:else}
            <pre class="mt-3 max-h-48 overflow-auto rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">
{JSON.stringify(approval.args, null, 2)}
            </pre>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}
