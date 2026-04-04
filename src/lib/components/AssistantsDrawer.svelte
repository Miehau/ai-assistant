<script lang="ts">
  import { ChevronLeft } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import Assistants from "$lib/components/Assistants.svelte";

  export let isOpen = false;
  export let onBack: (() => void) | undefined = undefined;

  function closeDrawer() {
    isOpen = false;
    onBack?.();
  }
</script>

<div
  class="fixed inset-0 z-40 bg-black/45 backdrop-blur-sm drawer-overlay"
  class:drawer-overlay-open={isOpen}
  onclick={(e) => { e.stopPropagation(); isOpen = false; }}
  role="button"
  tabindex={isOpen ? 0 : -1}
  aria-hidden={!isOpen}
  onkeydown={(e) => { if (e.key === "Escape") isOpen = false; }}
></div>
<div
  class="fixed top-8 bottom-0 left-0 z-50 w-[75vw] max-w-[75vw] glass-panel-minimal rounded-r-2xl border-r border-white/10 shadow-2xl flex flex-col overflow-hidden drawer-panel"
  class:drawer-panel-open={isOpen}
  aria-hidden={!isOpen}
>
  <div class="flex items-center justify-between border-b border-white/10 px-5 py-4 shrink-0">
    <span class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Assistants</span>
    <Button variant="ghost" size="icon" class="rounded-lg" onclick={closeDrawer} aria-label="Back">
      <ChevronLeft class="size-4" />
    </Button>
  </div>
  <div class="flex-1 overflow-y-auto">
    {#if isOpen}
      <Assistants />
    {/if}
  </div>
</div>
