<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { goto } from "$app/navigation";
  import { modelsDrawerOpen } from "$lib/stores/drawers";
  import { lastNonSettingsPath } from "$lib/stores/navigation";

  onMount(() => {
    modelsDrawerOpen.set(true);
    const target = get(lastNonSettingsPath) || "/";
    void goto(target, { replaceState: true });
  });
</script>

<div class="p-6 text-sm text-muted-foreground">Opening models...</div>
