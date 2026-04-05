<script lang="ts">
  import { onMount } from "svelte";
  import { conversationService } from "$lib/services/conversation";
  import type { Conversation } from "$lib/types";
  import { formatDistanceToNow } from "date-fns";
  import { messages, isFirstMessage, loadConversationHistory } from "$lib/stores/chat";
  import { ChevronLeft, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { goto } from "$app/navigation";
  import SvelteVirtualList from "@humanspeak/svelte-virtual-list";
  import { CONVERSATION_ITEM_HEIGHT } from "$lib/utils/virtualHeights";

  export let isOpen = false;
  export let onBack: (() => void) | undefined = undefined;

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

  function closeWithBack() {
    isOpen = false;
    onBack?.();
  }

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
      await conversationService.setCurrentConversation(conversation.id);
      await loadConversationHistory(conversation.id);
      isOpen = false;
      if (window.location.pathname !== '/') {
        goto('/');
      }
    } catch (err) {
      console.error("Error selecting conversation:", err);
    }
  }

  async function deleteConversation(event: Event, conversationId: string) {
    event.stopPropagation();
    try {
      await conversationService.deleteConversation(conversationId);
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
    return conversation.name || `Conversation ${conversation.id.substring(0, 8)}...`;
  }
</script>

<div
  class="fixed inset-0 z-20 bg-black/40 drawer-overlay"
  class:drawer-overlay-open={isOpen}
  inert={!isOpen}
  onclick={(e) => { e.stopPropagation(); isOpen = false; }}
  role="button"
  tabindex={isOpen ? 0 : -1}
  aria-hidden={!isOpen}
  onkeydown={(e) => { if (e.key === 'Escape') isOpen = false; }}
></div>
<div
  class="fixed top-8 bottom-0 left-0 z-30 w-[360px] glass-panel border-r-0 shadow-2xl flex flex-col rounded-r-2xl overflow-hidden drawer-panel"
  class:drawer-panel-open={isOpen}
  inert={!isOpen}
  aria-hidden={!isOpen}
>
  <div class="flex items-center justify-between px-4 py-3 border-b border-white/10 bg-white/5 shrink-0">
    <div class="flex items-center gap-2">
      <span class="text-xs uppercase tracking-wide text-muted-foreground/70">Conversations</span>
      <span class="glass-badge-sm text-[10px] text-muted-foreground/70">{conversations.length}</span>
    </div>
    <Button variant="ghost" size="icon" onclick={closeWithBack}>
      <ChevronLeft class="h-4 w-4" />
    </Button>
  </div>

  <div class="flex-1 overflow-y-auto min-h-0 relative px-2 py-2">
    {#if isOpen && loading}
      <div class="flex justify-center items-center h-32">
        <div class="loading-spinner"></div>
      </div>
    {:else if isOpen && error}
      <div class="p-4 text-destructive glass-panel-minimal rounded-xl">
        <p>Error: {error}</p>
        <button
          class="text-sm text-primary mt-2 underline"
          onclick={loadConversations}
        >
          Try again
        </button>
      </div>
    {:else if isOpen && conversations.length === 0}
      <div class="p-6 text-muted-foreground text-center glass-panel-minimal rounded-xl">
        <p>No previous conversations found</p>
      </div>
    {:else if isOpen}
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
