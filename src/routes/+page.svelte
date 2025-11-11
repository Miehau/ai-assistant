<script lang="ts">
  import Chat from "$lib/components/Chat.svelte";
</script>

<div class="grid h-screen w-full relative overflow-hidden">
  <!-- Animated gradient background -->
  <div class="absolute inset-0 -z-10">
    <div class="absolute top-0 -left-4 w-96 h-96 bg-primary/10 rounded-full blur-3xl animate-pulse"></div>
    <div class="absolute top-20 right-10 w-80 h-80 bg-accent-cyan/10 rounded-full blur-3xl animate-pulse" style="animation-delay: 1s;"></div>
    <div class="absolute bottom-10 left-20 w-72 h-72 bg-accent-purple/10 rounded-full blur-3xl animate-pulse" style="animation-delay: 2s;"></div>
  </div>

  <div class="flex flex-col">
    <main class="grid flex-1 gap-4 overflow-auto p-4">
      <Chat />
    </main>
  </div>
</div>
