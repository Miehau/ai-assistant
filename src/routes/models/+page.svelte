<script lang="ts">
  import Models from "$lib/components/Models.svelte";
  import CustomBackends from "$lib/components/CustomBackends.svelte";
  import ServerModels from "$lib/components/ServerModels.svelte";
  import { honoBackend } from "$lib/stores/honoBackend.svelte";
</script>

{#if honoBackend.enabled}
  <ServerModels />
{:else}
  <Models />
  <CustomBackends />
{/if}
