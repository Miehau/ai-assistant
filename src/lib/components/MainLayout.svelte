<script lang="ts">
  import Navbar from "$lib/components/Navbar.svelte";
</script>

<div class="app-shell">
  <!-- Drag region for window movement (macOS traffic lights area) -->
  <div
    data-tauri-drag-region
    class="fixed top-0 left-0 right-0 h-8 z-40 cursor-move select-none"
  ></div>

  <header class="fixed top-0 left-0 right-0 z-10">
    <div class="glass-panel-minimal border-b border-white/10">
      <div class="flex h-12 items-center justify-between pl-[72px] pr-4">
        <div class="flex items-center gap-2 text-[11px] uppercase tracking-[0.28em] text-muted-foreground/80">
          <span class="inline-flex h-1.5 w-1.5 rounded-full bg-emerald-400/80 shadow-[0_0_10px_rgba(16,185,129,0.6)]"></span>
          AI Frontend
        </div>
        <div class="glass-badge-subtle px-2.5 py-1 text-[10px] uppercase tracking-[0.2em] text-muted-foreground/70">
          Local
        </div>
      </div>
    </div>
  </header>

  <Navbar />

  <div class="app-content pl-[72px] pt-12 h-screen overflow-y-auto">
    <slot />
  </div>
</div>
