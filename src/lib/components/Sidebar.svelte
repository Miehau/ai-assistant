<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import Icon from "@iconify/svelte";
  import { settingsDrawerOpen } from "$lib/stores/drawers";

  export let sidebarOpen = false;

  function handleNavigate(event: MouseEvent, path: string) {
    if (event.defaultPrevented) return;
    if (event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;

    event.preventDefault();
    if (path === $page.url.pathname) return;
    void goto(path);
  }

  function openSettings(event: MouseEvent) {
    event.preventDefault();
    $settingsDrawerOpen = true;
  }
</script>

<nav class:open={sidebarOpen}>
  <ul>
    <li class:active={$page.url.pathname === "/"}>
      <a href="/" onclick={(event) => handleNavigate(event, "/")}>
        <Icon icon="mdi:chat" />
        <span>Chat</span>
      </a>
    </li>
    <li class:active={$settingsDrawerOpen || $page.url.pathname === "/settings"}>
      <a href="/settings" onclick={openSettings}>
        <Icon icon="mdi:cog" />
        <span>Settings</span>
      </a>
    </li>
    <li class:active={$page.url.pathname === "/models"}>
      <a href="/models" onclick={(event) => handleNavigate(event, "/models")}>
        <Icon icon="mdi:cube-outline" />
        <span>Models</span>
      </a>
    </li>
  </ul>
</nav>

<style>
  /* ... existing styles ... */
</style>
