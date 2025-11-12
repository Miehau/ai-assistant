<script lang="ts">
  import { onMount } from "svelte";
  import { conversationService } from "$lib/services/conversation";
  import type { Conversation } from "$lib/types";
  import { formatDistanceToNow } from "date-fns";
  import { messages, isFirstMessage } from "$lib/components/chat/store";
  import { fly } from "svelte/transition";
  import { X, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { goto } from "$app/navigation";
  import SvelteVirtualList from "@humanspeak/svelte-virtual-list";
  import { CONVERSATION_ITEM_HEIGHT } from "$lib/utils/virtualHeights";

  export let isOpen = false;

  let conversations: Conversation[] = [];
  let loading = false;
  let error: string | null = null;

  // Watch for changes to isOpen
  $: if (isOpen) {
    loadConversations();
  }

  // Subscribe to conversation state changes to refresh the list when needed
  const unsubscribe = conversationService.subscribe(state => {
    if (isOpen) {
      loadConversations();
    }
  });

  onMount(() => {
    return () => {
      unsubscribe();
    };
  });

  async function loadConversations() {
    try {
      loading = true;
      console.log('Loading conversations for drawer');
      conversations = await conversationService.getAllConversations();
      console.log('Loaded conversations:', conversations);
      conversations.sort((a, b) => {
        return new Date(b.created_at).getTime() - 
               new Date(a.created_at).getTime();
      });
    } catch (err) {
      error = err instanceof Error ? err.message : "Failed to load conversations";
      console.error("Error loading conversations:", err);
    } finally {
      loading = false;
    }
  }

  async function selectConversation(conversation: Conversation) {
    try {
      // Set the selected conversation as current
      await conversationService.setCurrentConversation(conversation.id);
      
      // Load the conversation messages
      const loadedMessages = await conversationService.getDisplayHistory(conversation.id);
      $messages = loadedMessages;
      
      // Close the drawer after selection
      isOpen = false;
      
      // Navigate to the main page only if not already there
      if (window.location.pathname !== '/') {
        goto('/');
      }
    } catch (err) {
      console.error("Error selecting conversation:", err);
    }
  }

  async function deleteConversation(event: Event, conversationId: string) {
    // Stop event propagation to prevent selecting the conversation
    event.stopPropagation();
    
    try {
      await conversationService.deleteConversation(conversationId);
      // Refresh the conversation list
      loadConversations();
    } catch (err) {
      console.error("Error deleting conversation:", err);
    }
  }

  function formatDate(dateString: string): string {
    try {
      const date = new Date(dateString);
      return formatDistanceToNow(date, { addSuffix: true });
    } catch (e) {
      return "Unknown date";
    }
  }

  function getPreviewText(conversation: Conversation): string {
    // This would ideally come from the first few messages
    // For now, just return the name or ID
    return conversation.name || `Conversation ${conversation.id.substring(0, 8)}...`;
  }
</script>

{#if isOpen}
  <div 
    class="fixed inset-y-0 left-[58px] z-10 w-80 bg-background border-r shadow-lg"
    transition:fly={{ x: -320, duration: 200 }}
  >
    <div class="flex items-center justify-between p-4 border-b">
      <h2 class="text-lg font-semibold">Conversation History</h2>
      <Button variant="ghost" size="icon" on:click={() => isOpen = false}>
        <X class="h-4 w-4" />
      </Button>
    </div>
    
    <div class="overflow-y-auto h-[calc(100vh-64px)]">
      {#if loading}
        <div class="flex justify-center items-center h-32">
          <div class="loading-spinner"></div>
        </div>
      {:else if error}
        <div class="p-4 text-destructive">
          <p>Error: {error}</p>
          <button 
            class="text-sm text-primary mt-2 underline" 
            on:click={loadConversations}
          >
            Try again
          </button>
        </div>
      {:else if conversations.length === 0}
        <div class="p-4 text-muted-foreground text-center">
          <p>No previous conversations found</p>
        </div>
      {:else}
        <SvelteVirtualList
          items={conversations}
          defaultEstimatedItemHeight={CONVERSATION_ITEM_HEIGHT}
          containerClass="divide-y"
        >
          {#snippet renderItem(conversation)}
            <div class="relative group">
              <button
                class="w-full text-left p-4 hover:bg-muted transition-colors"
                on:click={() => selectConversation(conversation)}
              >
                <div class="font-medium truncate pr-8">{getPreviewText(conversation)}</div>
                <div class="text-xs text-muted-foreground mt-1">
                  {formatDate(conversation.created_at)}
                </div>
              </button>
              <button
                class="absolute right-3 top-1/2 transform -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity p-1 hover:bg-muted-foreground/10 rounded"
                on:click={(e) => deleteConversation(e, conversation.id)}
                title="Delete conversation"
              >
                <Trash2 class="h-4 w-4 text-muted-foreground hover:text-destructive" />
              </button>
            </div>
          {/snippet}
        </SvelteVirtualList>
      {/if}
    </div>
  </div>
{/if}

<style>
  .loading-spinner {
    display: inline-block;
    width: 24px;
    height: 24px;
    border: 3px solid rgba(0, 0, 0, 0.1);
    border-radius: 50%;
    border-top-color: var(--primary, #333);
    animation: spin 1s ease-in-out infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
