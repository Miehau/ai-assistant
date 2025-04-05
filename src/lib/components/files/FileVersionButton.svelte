<!-- 
  FileVersionButton.svelte - Button component for accessing file version history
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import FileVersionHistory from './FileVersionHistory.svelte';
  
  // Props
  export let filePath: string;
  
  // State
  let showVersionHistory = false;
  
  // Event dispatcher
  const dispatch = createEventDispatcher();
  
  // Handle version events
  function handleVersionEvent(event: CustomEvent) {
    // Forward events from the version history component
    dispatch(event.type, event.detail);
  }
</script>

<div class="relative">
  <button
    on:click={() => showVersionHistory = !showVersionHistory}
    class="px-2 py-1 text-xs bg-gray-200 text-gray-700 rounded hover:bg-gray-300 flex items-center"
    aria-label="Show version history"
  >
    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
    Versions
  </button>
  
  {#if showVersionHistory}
    <div class="absolute right-0 top-8 z-10 w-[800px] mt-2">
      <FileVersionHistory
        {filePath}
        showHistory={showVersionHistory}
        on:close={() => showVersionHistory = false}
        on:versionCreated={handleVersionEvent}
        on:versionRestored={handleVersionEvent}
        on:versionDeleted={handleVersionEvent}
        on:versionsCleanedUp={handleVersionEvent}
      />
    </div>
  {/if}
</div>
