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
  import { conversationService } from "$lib/services/conversation";

  let chatContainer: HTMLElement | null = null;
  let autoScroll = true;

  onMount(async () => {
    loadModels();
    loadSystemPrompts();

    // Initial load of messages if there's a current conversation
    const currentConversation = conversationService.getCurrentConversation();
    if (currentConversation) {
      const loadedMessages = await conversationService.getDisplayHistory(
        currentConversation.id,
      );
      $messages = loadedMessages;
    }
  });

  afterUpdate(() => {
    if (chatContainer) {
      scrollToBottom();
    }
  });

  function scrollToBottom() {
    if (chatContainer && autoScroll) {
      const newScrollTop = chatContainer.scrollHeight - chatContainer.clientHeight;
      chatContainer.scrollTop = newScrollTop;
    }
  }

  function handleSendMessage() {
    sendMessage();
  }

  function handleToggleStreaming() {
    toggleStreaming();
  }

  function handleClearConversation() {
    clearConversation();
  }
</script>

<div class="relative flex flex-col h-full min-h-[50vh] rounded-xl bg-muted/50 p-4 lg:col-span-2 w-full">
  <div class="flex-1 overflow-hidden mb-4">
    <ChatMessages messages={$messages} bind:chatContainer bind:autoScroll />
  </div>
  
  <div class="sticky bottom-0 bg-muted/50">
    <ChatInput 
      bind:currentMessage={$currentMessage}
      bind:attachments={$attachments}
      isLoading={$isLoading}
      on:sendMessage={handleSendMessage}
    >
      <div slot="controls">
        <ChatControls
          availableModels={$availableModels}
          systemPrompts={$systemPrompts}
          bind:selectedModel={$selectedModel}
          bind:selectedSystemPrompt={$selectedSystemPrompt}
          bind:streamingEnabled={$streamingEnabled}
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
</style>
