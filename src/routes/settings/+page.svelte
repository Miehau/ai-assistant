<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { goto } from "$app/navigation";
  import { settingsDrawerOpen } from "$lib/stores/drawers";
  import { lastNonSettingsPath } from "$lib/stores/navigation";

  onMount(() => {
    settingsDrawerOpen.set(true);
    const target = get(lastNonSettingsPath) || "/";
    if (target === "/settings") {
      void goto("/", { replaceState: true });
      return;
    }
    void goto(target, { replaceState: true });
  });
</script>

<div class="p-6 text-sm text-muted-foreground">Opening settings...</div>
