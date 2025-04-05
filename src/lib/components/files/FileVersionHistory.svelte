<!-- 
  FileVersionHistory.svelte - Component for displaying and managing file version history
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { fileVersionService, type VersionMetadata } from '$lib/services/file-version';
  import { formatDistanceToNow } from 'date-fns';
  import { createEventDispatcher } from 'svelte';
  
  // Props
  export let filePath: string;
  export let showHistory = false;
  
  // State
  let versions: VersionMetadata[] = [];
  let currentVersionId = '';
  let loading = false;
  let error = '';
  let confirmDeleteId = '';
  
  // Event dispatcher
  const dispatch = createEventDispatcher();
  
  // Load version history when component is mounted or filePath changes
  $: if (filePath && showHistory) {
    loadVersionHistory();
  }
  
  async function loadVersionHistory() {
    loading = true;
    error = '';
    
    try {
      const result = await fileVersionService.getVersionHistory(filePath);
      
      if (result.success && result.history) {
        versions = result.history.versions;
        currentVersionId = result.history.current_version;
      } else {
        error = result.error || 'Failed to load version history';
        versions = [];
      }
    } catch (err) {
      console.error('Error loading version history:', err);
      error = `Error: ${err}`;
      versions = [];
    } finally {
      loading = false;
    }
  }
  
  // Format file size for display
  function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  
  // Format date for display
  function formatDate(dateStr: string): string {
    try {
      const date = new Date(dateStr);
      return formatDistanceToNow(date, { addSuffix: true });
    } catch (e) {
      return dateStr;
    }
  }
  
  // Restore a version
  async function restoreVersion(versionId: string) {
    loading = true;
    error = '';
    
    try {
      const result = await fileVersionService.restoreVersion(filePath, versionId);
      
      if (result.success) {
        // Reload version history after restore
        await loadVersionHistory();
        dispatch('versionRestored', { versionId });
      } else {
        error = result.error || 'Failed to restore version';
      }
    } catch (err) {
      console.error('Error restoring version:', err);
      error = `Error: ${err}`;
    } finally {
      loading = false;
    }
  }
  
  // Delete a version
  async function deleteVersion(versionId: string) {
    loading = true;
    error = '';
    confirmDeleteId = '';
    
    try {
      const result = await fileVersionService.deleteVersion(filePath, versionId);
      
      if (result.success) {
        // Reload version history after delete
        await loadVersionHistory();
        dispatch('versionDeleted', { versionId });
      } else {
        error = result.error || 'Failed to delete version';
      }
    } catch (err) {
      console.error('Error deleting version:', err);
      error = `Error: ${err}`;
    } finally {
      loading = false;
    }
  }
  
  // Create a new version
  async function createVersion() {
    const comment = prompt('Enter a comment for this version (optional):');
    
    loading = true;
    error = '';
    
    try {
      const result = await fileVersionService.createVersion(filePath, comment || undefined);
      
      if (result.success) {
        // Reload version history after creating new version
        await loadVersionHistory();
        dispatch('versionCreated', { version: result.version });
      } else {
        error = result.error || 'Failed to create version';
      }
    } catch (err) {
      console.error('Error creating version:', err);
      error = `Error: ${err}`;
    } finally {
      loading = false;
    }
  }
  
  // Clean up old versions
  async function cleanupVersions() {
    const keepCount = parseInt(prompt('How many recent versions would you like to keep?', '5') || '5');
    
    if (isNaN(keepCount) || keepCount < 1) {
      alert('Please enter a valid number greater than 0');
      return;
    }
    
    loading = true;
    error = '';
    
    try {
      const result = await fileVersionService.cleanupVersions(filePath, keepCount);
      
      if (result.success) {
        // Reload version history after cleanup
        await loadVersionHistory();
        dispatch('versionsCleanedUp', { deletedCount: result.deleted_count });
      } else {
        error = result.error || 'Failed to clean up versions';
      }
    } catch (err) {
      console.error('Error cleaning up versions:', err);
      error = `Error: ${err}`;
    } finally {
      loading = false;
    }
  }
</script>

{#if showHistory}
  <div class="version-history-container p-4 bg-gray-50 rounded-lg shadow-sm">
    <div class="flex justify-between items-center mb-4">
      <h3 class="text-lg font-semibold">Version History</h3>
      <div class="space-x-2">
        <button 
          on:click={createVersion}
          disabled={loading}
          class="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50"
        >
          Create Version
        </button>
        <button 
          on:click={cleanupVersions}
          disabled={loading || versions.length <= 1}
          class="px-3 py-1 bg-gray-500 text-white rounded hover:bg-gray-600 disabled:opacity-50"
        >
          Clean Up
        </button>
        <button 
          on:click={() => dispatch('close')}
          class="px-3 py-1 bg-gray-300 text-gray-700 rounded hover:bg-gray-400"
        >
          Close
        </button>
      </div>
    </div>
    
    {#if error}
      <div class="bg-red-100 border border-red-300 text-red-700 px-4 py-2 rounded mb-4">
        {error}
      </div>
    {/if}
    
    {#if loading}
      <div class="flex justify-center py-8">
        <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
      </div>
    {:else if versions.length === 0}
      <div class="text-center py-8 text-gray-500">
        No version history available for this file.
      </div>
    {:else}
      <div class="overflow-x-auto">
        <table class="min-w-full bg-white">
          <thead>
            <tr class="bg-gray-200 text-gray-600 uppercase text-sm leading-normal">
              <th class="py-3 px-6 text-left">Version</th>
              <th class="py-3 px-6 text-left">Created</th>
              <th class="py-3 px-6 text-left">Size</th>
              <th class="py-3 px-6 text-left">Comment</th>
              <th class="py-3 px-6 text-center">Actions</th>
            </tr>
          </thead>
          <tbody class="text-gray-600 text-sm">
            {#each versions as version (version.version_id)}
              <tr class="border-b border-gray-200 hover:bg-gray-100">
                <td class="py-3 px-6 text-left">
                  <div class="flex items-center">
                    {#if version.version_id === currentVersionId}
                      <span class="bg-green-100 text-green-800 text-xs font-medium mr-2 px-2.5 py-0.5 rounded">Current</span>
                    {/if}
                    {version.version_id}
                  </div>
                </td>
                <td class="py-3 px-6 text-left">{formatDate(version.created_at)}</td>
                <td class="py-3 px-6 text-left">{formatFileSize(version.file_size)}</td>
                <td class="py-3 px-6 text-left">{version.comment || '-'}</td>
                <td class="py-3 px-6 text-center">
                  <div class="flex item-center justify-center space-x-2">
                    {#if version.version_id !== currentVersionId}
                      <button 
                        on:click={() => restoreVersion(version.version_id)}
                        disabled={loading}
                        class="px-2 py-1 bg-blue-500 text-white text-xs rounded hover:bg-blue-600 disabled:opacity-50"
                      >
                        Restore
                      </button>
                      
                      {#if confirmDeleteId === version.version_id}
                        <button 
                          on:click={() => deleteVersion(version.version_id)}
                          disabled={loading}
                          class="px-2 py-1 bg-red-500 text-white text-xs rounded hover:bg-red-600 disabled:opacity-50"
                        >
                          Confirm
                        </button>
                        <button 
                          on:click={() => confirmDeleteId = ''}
                          disabled={loading}
                          class="px-2 py-1 bg-gray-300 text-gray-700 text-xs rounded hover:bg-gray-400 disabled:opacity-50"
                        >
                          Cancel
                        </button>
                      {:else}
                        <button 
                          on:click={() => confirmDeleteId = version.version_id}
                          disabled={loading}
                          class="px-2 py-1 bg-gray-500 text-white text-xs rounded hover:bg-gray-600 disabled:opacity-50"
                        >
                          Delete
                        </button>
                      {/if}
                    {:else}
                      <span class="text-xs text-gray-500">Current version</span>
                    {/if}
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
{/if}

<style>
  .version-history-container {
    max-height: 500px;
    overflow-y: auto;
  }
</style>
