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
  import { branchService } from "$lib/services/branchService";
  import { branchStore } from "$lib/stores/branches";
  import { fade } from "svelte/transition";
  import { debugModels } from "./debug";

  let chatContainer: HTMLElement | null = null;
  let autoScroll = true;
  let isClearing = false;

  // Track previous IDs to prevent infinite loops
  let previousConversationId: string | null = null;
  let previousBranchId: string | null = null;
  let isLoadingBranches = false;
  let isLoadingMessages = false;

  // Load branches for the current conversation
  async function loadBranches(conversationId: string) {
    // Prevent re-entry and infinite loops
    if (isLoadingBranches || previousConversationId === conversationId) {
      return;
    }

    try {
      isLoadingBranches = true;
      previousConversationId = conversationId;

      const branches = await branchService.getConversationBranches(conversationId);
      const mainBranch = branches.find(b => b.name === 'Main') || branches[0];
      branchStore.setBranches(branches);
      if (mainBranch) {
        branchStore.setCurrentBranch(mainBranch.id);
      }
      console.log('Loaded branches:', branches);
    } catch (error) {
      console.error('Failed to load branches:', error);
    } finally {
      isLoadingBranches = false;
    }
  }

  // Load messages for a specific branch
  async function loadBranchMessages(branchId: string) {
    // Prevent re-entry and infinite loops
    if (isLoadingMessages || previousBranchId === branchId) {
      return;
    }

    try {
      isLoadingMessages = true;
      previousBranchId = branchId;

      const branchPath = await branchService.getBranchPath(branchId);

      // Convert to display format
      const branchMessages = branchPath.messages.map(msg => ({
        id: msg.id,
        type: msg.role === 'user' ? 'sent' : 'received',
        content: msg.content,
        attachments: msg.attachments || []
      }));

      $messages = branchMessages;
      console.log(`Switched to branch "${branchPath.branch.name}" with ${branchMessages.length} messages`);
    } catch (error) {
      console.error('Failed to load branch messages:', error);
    } finally {
      isLoadingMessages = false;
    }
  }

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
      // Initialize branch context for existing conversation
      await chatService.initializeBranchContext(currentConversation.id);

      // Load branches for this conversation
      await loadBranches(currentConversation.id);

      // Load messages for the current branch
      if ($branchStore.currentBranchId) {
        await loadBranchMessages($branchStore.currentBranchId);
      } else {
        // Fallback: load all messages if no branch set
        const loadedMessages = await conversationService.getDisplayHistory(
          currentConversation.id
        );
        $messages = loadedMessages;
      }
    }
  });

  // Watch for conversation changes and reload branches
  $: if ($currentConversation?.id) {
    loadBranches($currentConversation.id);
  }

  // Watch for branch changes and reload messages
  $: if ($branchStore.currentBranchId && $currentConversation?.id) {
    loadBranchMessages($branchStore.currentBranchId);
  }

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

      // Reset tracking IDs to allow fresh loads
      previousConversationId = null;
      previousBranchId = null;

      // Reset the clearing state almost immediately - just enough time for a brief visual feedback
      setTimeout(() => {
        isClearing = false;
      }, 200); // Very short delay for visual feedback
    } else {
      clearConversation();
      previousConversationId = null;
      previousBranchId = null;
    }
  }
</script>

<div class="relative flex flex-col h-full min-h-[50vh] max-h-screen rounded-xl bg-muted/50 p-4 lg:col-span-2 w-full">
  <div class="flex-1 overflow-auto mb-4 relative">
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

  <div class="bg-muted/50">
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
