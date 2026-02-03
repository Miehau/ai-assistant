<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import Settings from "$lib/components/Settings.svelte";

  export let isOpen = false;

  function closeDrawer() {
    isOpen = false;
  }
</script>

{#if isOpen}
  <div
    class="fixed inset-0 z-40 bg-black/45 backdrop-blur-sm"
    onclick={closeDrawer}
    transition:fade={{ duration: 150 }}
    role="button"
    tabindex="0"
    onkeydown={(e) => e.key === "Escape" && closeDrawer()}
  ></div>
  <div
    class="fixed inset-y-0 left-0 z-50 w-[75vw] max-w-[75vw] glass-panel-minimal rounded-r-2xl border-r border-white/10 shadow-2xl flex flex-col overflow-hidden"
    transition:fly={{ x: -600, duration: 220 }}
  >
    <Settings showClose={true} onClose={closeDrawer} />
  </div>
{/if}
