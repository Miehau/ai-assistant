<script lang="ts">
  import { page } from '$app/stores';
  import { slide } from 'svelte/transition';
  import Icon from '@iconify/svelte';

  export let sidebarOpen = false;
</script>

<nav class:open={sidebarOpen} transition:slide={{ duration: 300 }}>
  <ul>
    <li class:active={$page.url.pathname === '/'}>
      <a href="/">
        <Icon icon="mdi:chat" />
        <span>Chat</span>
      </a>
    </li>
    <li class:active={$page.url.pathname === '/settings'}>
      <a href="/settings">
        <Icon icon="mdi:cog" />
        <span>Settings</span>
      </a>
    </li>
    <li class:active={$page.url.pathname === '/models'}>
      <a href="/models">
        <Icon icon="mdi:cube-outline" />
        <span>Models</span>
      </a>
    </li>
  </ul>
</nav>

<style>
  /* ... existing styles ... */
</style>