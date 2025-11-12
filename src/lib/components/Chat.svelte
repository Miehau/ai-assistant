<script lang="ts">
  import { onMount, afterUpdate } from "svelte";
  import {
    messages,
    availableModels,
    systemPrompts,
    selectedModel,
    selectedSystemPrompt,
    streamingEnabled,
    isLoading,
    currentMessage,
    attachments,
    loadModels,
    loadSystemPrompts,
    toggleStreaming,
    sendMessage,
    clearConversation
  } from "./chat/store";
  import ChatMessages from "./chat/ChatMessages.svelte";
  import ChatInput from "./chat/ChatInput.svelte";
  import ChatControls from "./chat/ChatControls.svelte";
  import { conversationService, currentConversation } from "$lib/services/conversation";
  import { chatService } from "$lib/services/chat";
  import { fade } from "svelte/transition";
  import { debugModels } from "./debug";

  let chatContainer: HTMLElement | null = null;
  let autoScroll = true;
  let isClearing = false;

  onMount(async () => {
    await loadModels();
    loadSystemPrompts();

    // Debug models after loading
    setTimeout(() => {
      debugModels();
    }, 500);

    // Initial load of messages if there's a current conversation
    const currentConversation = conversationService.getCurrentConversation();
    if (currentConversation) {
      const loadedMessages = await conversationService.getDisplayHistory(
        currentConversation.id,
      );
      $messages = loadedMessages;

      // Initialize branch context for existing conversation
      await chatService.initializeBranchContext(currentConversation.id);
    }
  });

  function handleSendMessage() {
    sendMessage();
  }

  function handleToggleStreaming() {
    toggleStreaming();
  }

  function handleClearConversation() {
    if ($messages.length > 0) {
      isClearing = true;
      clearConversation();
      
      // Reset the clearing state almost immediately - just enough time for a brief visual feedback
      setTimeout(() => {
        isClearing = false;
      }, 200); // Very short delay for visual feedback
    } else {
      clearConversation();
    }
  }
</script>

<div class="relative flex flex-col h-full min-h-[50vh] rounded-xl bg-muted/50 p-4 lg:col-span-2 w-full">
  <div class="flex-1 overflow-hidden mb-4 relative">
    {#if isClearing}
      <div 
        class="absolute inset-0 flex items-center justify-center z-10 bg-background/30 backdrop-blur-sm" 
        transition:fade={{ duration: 200 }}
      >
        <div class="text-center">
          <div class="loading-spinner mb-2"></div>
          <p class="text-sm text-muted-foreground">Starting new conversation...</p>
        </div>
      </div>
    {/if}
    <ChatMessages messages={$messages} bind:chatContainer bind:autoScroll conversationId={$currentConversation?.id} />
  </div>
  
  <div class="sticky bottom-0 bg-muted/50">
    <ChatInput
      bind:currentMessage={$currentMessage}
      bind:attachments={$attachments}
      isLoading={$isLoading}
      modelId={$selectedModel}
      messages={$messages}
      systemPrompt={$selectedSystemPrompt?.content || ''}
      on:sendMessage={handleSendMessage}
    >
      <div slot="controls">
        <ChatControls
          availableModels={$availableModels}
          systemPrompts={$systemPrompts}
          bind:selectedModel={$selectedModel}
          bind:selectedSystemPrompt={$selectedSystemPrompt}
          bind:streamingEnabled={$streamingEnabled}
          conversationId={$currentConversation?.id}
          currentMessage={$currentMessage}
          messages={$messages}
          isLoading={$isLoading}
          on:toggleStreaming={handleToggleStreaming}
          on:removeMessages={handleClearConversation}
        />
      </div>
    </ChatInput>
  </div>
</div>

<style>
  /* Square attachment styles */
  :global(.square-attachment) {
    position: relative;
    width: 60px;
    height: 60px;
    border-radius: 8px;
    overflow: hidden;
    background-color: rgba(0, 0, 0, 0.05);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  :global(.square-attachment-thumbnail) {
    width: 100%;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  :global(.square-attachment-image) {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  :global(.square-attachment-icon-container) {
    width: 100%;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  :global(.square-attachment-icon) {
    width: 24px;
    height: 24px;
    color: rgba(0, 0, 0, 0.5);
  }

  :global(.square-attachment-name) {
    font-size: 10px;
    color: rgba(0, 0, 0, 0.7);
    margin-top: 4px;
    text-align: center;
    width: 100%;
    padding: 0 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global(.square-attachment-remove) {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background-color: rgba(0, 0, 0, 0.5);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0.7;
    transition: opacity 0.2s;
  }

  :global(.square-attachment-remove:hover) {
    opacity: 1;
  }
  
  /* Loading spinner */
  .loading-spinner {
    display: inline-block;
    width: 24px;
    height: 24px;
    border: 3px solid rgba(0, 0, 0, 0.1);
    border-radius: 50%;
    border-top-color: var(--primary, #333);
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
