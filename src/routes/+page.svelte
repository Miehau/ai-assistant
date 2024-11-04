<script lang="ts">
  
  import Chat from "$lib/components/Chat.svelte";
  import MainLayout from "$lib/components/MainLayout.svelte";
</script>

<MainLayout>
  <div class="grid h-screen w-full">
  <div class="flex flex-col">
    <main class="grid flex-1 gap-4 overflow-auto p-4 ">
      <Chat />
    </main>
  </div>
  </div>
</MainLayout>
