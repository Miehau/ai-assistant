<script lang="ts">
  import Navbar from "$lib/components/Navbar.svelte";
</script>

<!-- Drag region for window movement (macOS traffic lights area) -->
<div data-tauri-drag-region class="fixed top-0 left-0 right-0 h-8 z-50 pointer-events-none"></div>

<Navbar />

<div class="pl-[26px] h-screen overflow-hidden">
  <slot />
</div>
