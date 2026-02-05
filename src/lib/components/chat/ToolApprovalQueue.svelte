<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import type {
    ToolExecutionApprovalScope,
    ToolExecutionProposedPayload
  } from "$lib/types/events";
  import { resolveToolApproval } from "$lib/stores/chat";

  export let approvals: ToolExecutionProposedPayload[] = [];
  export let containerClass = "";

  function approve(approvalId: string, scope: ToolExecutionApprovalScope) {
    resolveToolApproval(approvalId, true, scope);
  }

  function deny(approvalId: string) {
    resolveToolApproval(approvalId, false);
  }

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

  function looksLikeDiff(text: string): boolean {
    return text.startsWith("--- a/") || text.includes("\n@@") || text.includes("\ndiff --git ");
  }

  function splitDiffLines(diff: string): string[] {
    const lines = diff.replace(/\n$/, "").split("\n");
    return lines.length === 1 && lines[0] === "" ? [] : lines;
  }

  function diffLines(preview: ToolExecutionProposedPayload["preview"]): string[] | null {
    if (!preview) return null;
    if (typeof preview === "string") {
      return looksLikeDiff(preview) ? splitDiffLines(preview) : null;
    }
    if (typeof preview === "object" && preview && "diff" in preview) {
      const diff = (preview as { diff?: unknown }).diff;
      if (typeof diff === "string") {
        return splitDiffLines(diff);
      }
    }
    return null;
  }

  const isDiffHeader = (line: string) => line.startsWith("diff --git ");
  const isFileHeader = (line: string) => line.startsWith("--- ") || line.startsWith("+++ ");
  const isHunkHeader = (line: string) => line.startsWith("@@");
  const isAddedLine = (line: string) => line.startsWith("+") && !line.startsWith("+++");
  const isRemovedLine = (line: string) => line.startsWith("-") && !line.startsWith("---");

  function previewTitle(preview: ToolExecutionProposedPayload["preview"]): string | null {
    if (!preview || typeof preview !== "object") return null;
    if ("path" in preview && typeof (preview as { path?: unknown }).path === "string") {
      return (preview as { path: string }).path;
    }
    return null;
  }
</script>

{#if approvals.length > 0}
  <div class={`mb-4 w-full rounded-2xl border border-border/60 bg-background/60 backdrop-blur p-4 shadow-sm ${containerClass}`}>
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
            <div class="flex flex-wrap items-center justify-end gap-2">
              <Button
                size="sm"
                onclick={() => approve(approval.approval_id, "once")}
              >
                Approve once (recommended)
              </Button>
              <Button
                variant="outline"
                size="sm"
                onclick={() => approve(approval.approval_id, "conversation")}
              >
                Approve for this conversation
              </Button>
              <Button
                variant="outline"
                size="sm"
                onclick={() => approve(approval.approval_id, "always")}
              >
                Always approve this tool
              </Button>
              <Button
                variant="outline"
                size="sm"
                onclick={() => deny(approval.approval_id)}
              >
                Deny
              </Button>
            </div>
          </div>

          {#if approval.preview}
            {@const lines = diffLines(approval.preview)}
            {#if lines && lines.length > 0}
              <div class="mt-3 max-h-64 overflow-auto rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">
                {#each lines as line}
                  <div
                    class={`whitespace-pre px-1 ${isAddedLine(line) ? 'bg-primary/10' : ''} ${isRemovedLine(line) ? 'bg-destructive/10' : ''}`}
                    class:text-muted-foreground={isDiffHeader(line)}
                    class:text-accent-amber={isFileHeader(line)}
                    class:text-accent-cyan={isHunkHeader(line)}
                    class:font-semibold={isHunkHeader(line)}
                    class:text-primary={isAddedLine(line)}
                    class:text-destructive={isRemovedLine(line)}
                  >{line}</div>
                {/each}
              </div>
            {:else}
              <pre class="mt-3 max-h-64 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">{formatPreview(approval.preview)}</pre>
            {/if}
          {:else}
            <pre class="mt-3 max-h-48 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-lg bg-muted/40 p-3 text-xs font-mono text-foreground">{JSON.stringify(approval.args, null, 2)}</pre>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}
