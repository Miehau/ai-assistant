<script lang="ts">
  import { onMount } from "svelte";
  import { conversationService } from "$lib/services/conversation";
  import type { Conversation } from "$lib/types";
  import { formatDistanceToNow } from "date-fns";
  import { messages, isFirstMessage } from "$lib/stores/chat";
  import { fly, fade } from "svelte/transition";
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
    class="fixed inset-0 z-20 bg-black/40 backdrop-blur-sm"
    onclick={() => (isOpen = false)}
    transition:fade={{ duration: 150 }}
    role="button"
    tabindex="0"
    onkeydown={(e) => e.key === 'Escape' && (isOpen = false)}
  ></div>
  <div
    class="fixed inset-y-0 left-[58px] z-30 w-[360px] glass-panel border-r-0 shadow-2xl flex flex-col rounded-r-2xl overflow-hidden"
    transition:fly={{ x: -360, duration: 200 }}
  >
    <div class="flex items-center justify-between px-4 py-3 border-b border-white/10 bg-white/5 shrink-0">
      <div class="flex items-center gap-2">
        <span class="text-xs uppercase tracking-wide text-muted-foreground/70">Conversations</span>
        <span class="glass-badge-sm text-[10px] text-muted-foreground/70">{conversations.length}</span>
      </div>
      <Button variant="ghost" size="icon" onclick={() => (isOpen = false)}>
        <X class="h-4 w-4" />
      </Button>
    </div>

    <div class="flex-1 overflow-y-auto min-h-0 relative px-2 py-2">
      {#if loading}
        <div class="flex justify-center items-center h-32">
          <div class="loading-spinner"></div>
        </div>
      {:else if error}
        <div class="p-4 text-destructive glass-panel-minimal rounded-xl">
          <p>Error: {error}</p>
          <button
            class="text-sm text-primary mt-2 underline"
            onclick={loadConversations}
          >
            Try again
          </button>
        </div>
      {:else if conversations.length === 0}
        <div class="p-6 text-muted-foreground text-center glass-panel-minimal rounded-xl">
          <p>No previous conversations found</p>
        </div>
      {:else}
        <SvelteVirtualList
          items={conversations}
          defaultEstimatedItemHeight={CONVERSATION_ITEM_HEIGHT}
          containerClass="flex flex-col gap-2"
        >
          {#snippet renderItem(conversation)}
            <div class="relative group glass-panel-minimal rounded-xl transition-all duration-200 hover:glass-light">
              <button
                class="w-full text-left px-4 py-3"
                onclick={() => selectConversation(conversation)}
              >
                <div class="font-medium truncate pr-10">{getPreviewText(conversation)}</div>
                <div class="text-[11px] text-muted-foreground/70 mt-1">
                  {formatDate(conversation.created_at)}
                </div>
              </button>
              <button
                class="absolute right-3 top-3 opacity-0 group-hover:opacity-100 transition-opacity p-1 hover:bg-muted-foreground/10 rounded"
                onclick={(e) => deleteConversation(e, conversation.id)}
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
