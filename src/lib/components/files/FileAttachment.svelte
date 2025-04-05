<!-- 
  FileAttachment.svelte - Component for displaying file attachments with version control
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import FileVersionButton from './FileVersionButton.svelte';
  
  // Props
  export let attachment: any;
  export let showVersioning = true;
  
  // State
  let isLoading = true;
  let hasError = false;
  let isImageModalOpen = false;
  let fullImageSrc = '';
  
  // Modal portal container
  let modalPortal: HTMLDivElement | null = null;
  
  // Handle loading states
  let loadedThumbnail = false;
  let loadedImage = false;
  
  onMount(async () => {
    if (attachment.attachment_type === 'image') {
      try {
        // Check if we already have data in the attachment
        if (attachment.data) {
          console.log('Using attachment data');
          fullImageSrc = `${attachment.data}`;
          loadedImage = true;
        } else if (attachment.thumbnail) {
          console.log('Using attachment thumbnail');
          loadedThumbnail = true;
        } else if (attachment.path) {
          console.log('Loading from path:', attachment.path);
          // If we have a path but no data, try to load the file directly
          fullImageSrc = `tauri://localhost/${attachment.path}`;
          loadedImage = true;
        } else if (attachment.file_path) {
          console.log('Loading from file_path:', attachment.file_path);
          fullImageSrc = `tauri://localhost/${attachment.file_path}`;
          loadedImage = true;
        } else {
          console.log('No direct data, attempting to load thumbnail');
          // Load thumbnail first for faster display
          await loadThumbnail();
        }
      } catch (error) {
        console.error('Error handling image:', error);
        hasError = true;
      }
    }
    
    isLoading = false;
  });
  
  async function loadThumbnail() {
    try {
      // First check if the attachment already has a thumbnail
      if (attachment.thumbnail) {
        console.log('Using existing thumbnail from attachment');
        loadedThumbnail = true;
        return;
      }
      
      // If not, try to load it from the backend
      const filePath = attachment.path || attachment.file_path;
      if (filePath) {
        console.log('Loading thumbnail from backend for path:', filePath);
        const thumbnailData = await invoke('get_image_thumbnail', {
          filePath
        });
        
        if (thumbnailData) {
          console.log('Thumbnail loaded successfully');
          loadedThumbnail = true;
        } else {
          console.log('No thumbnail data returned from backend');
        }
      } else {
        console.log('No path available to load thumbnail');
      }
    } catch (error) {
      console.error('Error loading thumbnail:', error);
    }
  }
  
  async function loadFullImage() {
    if (loadedImage) return;
    
    try {
      // If the attachment already has data, use it
      if (attachment.data) {
        fullImageSrc = `${attachment.data}`;
        loadedImage = true;
        return;
      }
      
      // Otherwise, load from the backend
      if (attachment.path) {
        const imageData = await invoke('get_file', {
          filePath: attachment.path
        });
        
        if (imageData) {
          fullImageSrc = `${imageData}`;
          loadedImage = true;
        }
      }
    } catch (error) {
      console.error('Error loading full image:', error);
      hasError = true;
    }
  }
  
  function openImageModal() {
    loadFullImage();
    
    // Create modal portal if it doesn't exist
    if (!modalPortal) {
      modalPortal = document.createElement('div');
      modalPortal.className = 'modal-portal';
      document.body.appendChild(modalPortal);
    }
    
    // Render modal content to the portal
    renderModalContent();
    
    isImageModalOpen = true;
    // Prevent scrolling on body when modal is open
    document.body.style.overflow = 'hidden';
  }
  
  function closeImageModal() {
    isImageModalOpen = false;
    // Re-enable scrolling when modal is closed
    document.body.style.overflow = '';
    
    // Clear modal content
    if (modalPortal) {
      modalPortal.innerHTML = '';
    }
  }
  
  // Render modal content to the portal
  function renderModalContent() {
    if (!modalPortal) return;
    
    const modalHTML = `
      <div class="modal-container">
        <button 
          class="fixed inset-0 z-[100000] w-full h-full bg-black/80 border-0" 
          id="modal-backdrop"
          aria-label="Close image modal"
        ></button>
        <div 
          class="fixed inset-0 z-[100001] flex items-center justify-center pointer-events-none" 
          role="dialog"
          aria-modal="true"
          aria-labelledby="modal-title"
        >
          <div class="relative max-w-4xl max-h-[90vh] pointer-events-auto">
            <button
              type="button"
              class="absolute top-4 right-4 text-white bg-black/50 rounded-full p-2 hover:bg-black/70"
              id="modal-close-btn"
              aria-label="Close modal"
            >
              <svg xmlns="http://www.w3.org/2000/svg" class="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
            <img
              src="${fullImageSrc || (attachment.data ? `${attachment.data}` : (attachment.path ? `tauri://localhost/${attachment.path}` : (attachment.file_path ? `tauri://localhost/${attachment.file_path}` : '')))}"
              alt="${attachment.name}"
              class="max-w-full max-h-[85vh] object-contain"
            />
            <div class="absolute bottom-4 left-4 right-4 flex justify-between items-center">
              <span class="text-white bg-black/50 px-3 py-1 rounded">${attachment.name}</span>
            </div>
          </div>
        </div>
      </div>
    `;
    
    modalPortal.innerHTML = modalHTML;
    
    // Add event listeners
    document.getElementById('modal-backdrop')?.addEventListener('click', closeImageModal);
    document.getElementById('modal-close-btn')?.addEventListener('click', closeImageModal);
  }
  
  // Clean up portal on component destruction
  onDestroy(() => {
    if (modalPortal && document.body.contains(modalPortal)) {
      document.body.removeChild(modalPortal);
    }
  });
  
  // Handle version events
  function handleVersionEvent(event: CustomEvent) {
    // Reload the attachment after version changes
    if (event.type === 'versionRestored') {
      loadedImage = false;
      loadedThumbnail = false;
      isLoading = true;
      
      // Reload the attachment
      setTimeout(() => {
        loadThumbnail();
        isLoading = false;
      }, 500);
    }
  }
</script>

<style>
  /* Modal styles */
  :global(.modal-portal) {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    z-index: 999999; /* Ultra high z-index */
    pointer-events: none;
  }
  
  :global(.modal-portal .modal-container) {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    z-index: 999999; /* Ultra high z-index */
    pointer-events: auto;
  }
</style>

<div class="file-attachment">
  {#if attachment.attachment_type === 'image'}
    <div class="relative">
      <!-- Show loading indicator while image is loading -->
      {#if isLoading}
        <div class="absolute inset-0 flex items-center justify-center bg-background/50 rounded-xl">
          <div class="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
        </div>
      {/if}
      
      <!-- Show thumbnail or a placeholder while full image is loading -->
      {#if (loadedThumbnail || attachment.thumbnail) && !loadedImage}
        <button
          type="button"
          class="p-0 border-0 bg-transparent"
          on:click={openImageModal}
          aria-label="View full image"
        >
          {#if attachment.thumbnail}
            <!-- Use base64 thumbnail data if available -->
            <img
              src={`${attachment.thumbnail}`}
              alt={attachment.name}
              class="max-w-xs rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
            />
          {:else if attachment.path}
            <!-- Use direct path if available -->
            <img
              src={`tauri://localhost/${attachment.path}`}
              alt={attachment.name}
              class="max-w-xs rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
            />
          {:else if attachment.file_path}
            <!-- Use file_path as fallback -->
            <img
              src={`tauri://localhost/${attachment.file_path}`}
              alt={attachment.name}
              class="max-w-xs rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
            />
          {:else}
            <!-- Placeholder if nothing else is available -->
            <div class="flex items-center justify-center bg-gray-100 rounded-xl w-64 h-48">
              <svg class="w-12 h-12 text-gray-400" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
            </div>
          {/if}
        </button>
      {/if}
      
      <!-- Show full image once loaded -->
      {#if loadedImage || attachment.data}
        <button
          type="button"
          class="p-0 border-0 bg-transparent"
          on:click={openImageModal}
          aria-label="View full image"
        >
          {#if fullImageSrc}
            <img
              src={fullImageSrc}
              alt={attachment.name}
              class="max-w-xs rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
            />
          {:else if attachment.data}
            <img
              src={`${attachment.data}`}
              alt={attachment.name}
              class="max-w-xs rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
            />
          {:else if attachment.path}
            <img
              src={`tauri://localhost/${attachment.path}`}
              alt={attachment.name}
              class="max-w-xs rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
            />
          {:else if attachment.file_path}
            <img
              src={`tauri://localhost/${attachment.file_path}`}
              alt={attachment.name}
              class="max-w-xs rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
            />
          {/if}
        </button>
      {/if}
      
      <!-- Error state -->
      {#if hasError}
        <div class="bg-red-100 border border-red-300 text-red-700 px-4 py-2 rounded">
          Failed to load image: {attachment.name}
        </div>
      {/if}
      
    </div>
  {:else if attachment.attachment_type === 'audio'}
    <div class="p-3 border rounded-xl bg-gray-50">
      <div class="flex items-center justify-between mb-2">
        <span class="text-sm font-medium">{attachment.name}</span>
        
        {#if showVersioning}
          <FileVersionButton 
            filePath={attachment.path}
            on:versionCreated={handleVersionEvent}
            on:versionRestored={handleVersionEvent}
            on:versionDeleted={handleVersionEvent}
            on:versionsCleanedUp={handleVersionEvent}
          />
        {/if}
      </div>
      <audio controls class="w-full">
        <source src={`tauri://localhost/${attachment.path}`} type={attachment.mime_type} />
        Your browser does not support the audio element.
      </audio>
    </div>
  {:else if attachment.attachment_type === 'text'}
    <div class="p-3 border rounded-xl bg-gray-50">
      <div class="flex items-center justify-between mb-2">
        <span class="text-sm font-medium">{attachment.name}</span>
        
        {#if showVersioning}
          <FileVersionButton 
            filePath={attachment.path}
            on:versionCreated={handleVersionEvent}
            on:versionRestored={handleVersionEvent}
            on:versionDeleted={handleVersionEvent}
            on:versionsCleanedUp={handleVersionEvent}
          />
        {/if}
      </div>
      <div class="text-sm max-h-48 overflow-y-auto">
        <pre class="whitespace-pre-wrap">{attachment.content || 'No content available'}</pre>
      </div>
    </div>
  {:else}
    <!-- Generic file attachment -->
    <div class="p-3 border rounded-xl bg-gray-50">
      <div class="flex items-center justify-between">
        <div class="flex items-center">
          <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 mr-2 text-gray-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <span class="text-sm font-medium">{attachment.name}</span>
        </div>
        
        {#if showVersioning}
          <FileVersionButton 
            filePath={attachment.path}
            on:versionCreated={handleVersionEvent}
            on:versionRestored={handleVersionEvent}
            on:versionDeleted={handleVersionEvent}
            on:versionsCleanedUp={handleVersionEvent}
          />
        {/if}
      </div>
    </div>
  {/if}
</div>
