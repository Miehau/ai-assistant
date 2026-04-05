<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { goto } from "$app/navigation";
  import { assistantsDrawerOpen } from "$lib/stores/drawers";
  import { lastNonSettingsPath } from "$lib/stores/navigation";

  onMount(() => {
    assistantsDrawerOpen.set(true);
    const target = get(lastNonSettingsPath) || "/";
    void goto(target, { replaceState: true });
  });
</script>

<div class="p-6 text-sm text-muted-foreground">Opening assistants...</div>
