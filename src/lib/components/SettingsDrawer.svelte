<script lang="ts">
  import Settings from "$lib/components/Settings.svelte";

  export let isOpen = false;
  export let onBack: (() => void) | undefined = undefined;

  function closeDrawer() {
    isOpen = false;
    onBack?.();
  }
</script>

<div
  class="fixed inset-0 z-40 bg-black/45 drawer-overlay"
  class:drawer-overlay-open={isOpen}
  inert={!isOpen}
  onclick={(e) => { e.stopPropagation(); isOpen = false; }}
  role="button"
  tabindex={isOpen ? 0 : -1}
  aria-hidden={!isOpen}
  onkeydown={(e) => { if (e.key === "Escape") isOpen = false; }}
></div>
<div
  class="fixed top-8 bottom-0 left-0 z-50 w-[75vw] max-w-[75vw] glass-panel-minimal rounded-r-2xl border-r border-white/10 shadow-2xl flex flex-col overflow-hidden drawer-panel"
  class:drawer-panel-open={isOpen}
  inert={!isOpen}
  aria-hidden={!isOpen}
>
  {#if isOpen}
    <Settings showClose={true} onClose={closeDrawer} />
  {/if}
</div>
