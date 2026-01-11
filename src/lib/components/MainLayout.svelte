<script lang="ts">
  import { page } from "$app/stores";
  import Navbar from "$lib/components/Navbar.svelte";

  $: isChatRoute = $page.url.pathname === "/";
</script>

<div class="app-shell">
  <!-- Drag region for window movement (macOS traffic lights area) -->
  <div
    data-tauri-drag-region
    class="fixed top-0 left-0 right-0 h-8 z-40 cursor-move select-none"
  ></div>

  <header class="fixed top-0 left-0 right-0 z-30">
    <div class="glass-panel-minimal border-b border-white/10">
      <div class="h-8"></div>
    </div>
  </header>

  <Navbar />

  <div class="app-content pl-[72px] pt-8 h-screen min-h-0 {isChatRoute ? 'overflow-hidden' : 'overflow-y-auto'}">
    <slot />
  </div>
</div>
