<script lang="ts">
  import "../app.css";
  import MainLayout from "$lib/components/MainLayout.svelte";
</script>

<!-- Add data-sveltekit-preload-data and data-sveltekit-reload="off" to the root element -->
<MainLayout>
    <slot />
</MainLayout>
