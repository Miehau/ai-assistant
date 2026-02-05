<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import ChatMessage from "../ChatMessage.svelte";
  import { fade, fly, scale } from "svelte/transition";
  import { backOut } from "svelte/easing";
  import type { Message } from "$lib/types";
  import { streamingMessage, isStreaming } from "$lib/stores/chat";
  import { pageVisible } from "$lib/stores/visibility";
  import ToolApprovalQueue from "./ToolApprovalQueue.svelte";
  import ToolCallBubble from "./ToolCallBubble.svelte";
  import type { ToolExecutionProposedPayload } from "$lib/types/events";
  import { getPhaseLabel } from "$lib/types/agent";
  import type { AgentPlan, AgentPlanStep, PhaseKind } from "$lib/types/agent";
  import type { ToolActivityEntry } from "$lib/stores/chat";

  export let messages: Message[] = [];
  export let chatContainer: HTMLElement | null = null;
  export let autoScroll = true;
  export let conversationId: string | undefined = undefined;
  export let toolApprovals: ToolExecutionProposedPayload[] = [];
  export let toolActivity: ToolActivityEntry[] = [];
  export let agentPhase: PhaseKind | null = null;
  export let agentPlan: AgentPlan | null = null;
  export let agentPlanSteps: AgentPlanStep[] = [];
  export let isLoading = false;

  const INITIAL_VISIBLE_MESSAGES = 60;
  const LOAD_MORE_CHUNK = 40;
  const LOAD_MORE_THRESHOLD = 120;
  const ANIMATED_MESSAGE_LIMIT = 12;

  $: {
    // Keep optional props referenced to avoid unused export warnings.
    void toolActivity;
    void agentPlan;
    void agentPlanSteps;
  }

  $: phaseLabel = getPhaseLabel(agentPhase);
  $: showThinkingStatus =
    (isLoading || $isStreaming) &&
    !($isStreaming && $streamingMessage && $streamingMessage.length > 0);

  function shouldRenderMessage(msg: Message): boolean {
    if (msg.type === "sent") return true;
    if (msg.content.trim().length > 0) return true;
    return Boolean(msg.attachments && msg.attachments.length > 0);
  }

  let lastScrollHeight = 0;
  let lastScrollTop = 0;
  let scrollTimeout: NodeJS.Timeout | null = null;
  let lastMessageCount = 0;
  let scrollThrottleTimeout: number | null = null;
  let resizeThrottleTimeout: number | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let visibleCount = INITIAL_VISIBLE_MESSAGES;
  let visibleMessages: Message[] = [];
  let hasMoreMessages = false;
  let loadingMore = false;
  let lastTotalCount = 0;

  function preserveScrollFromBottom() {
    if (!chatContainer) return;

    const newScrollHeight = chatContainer.scrollHeight;
    const visibleHeight = chatContainer.clientHeight;

    // Calculate distance from bottom before resize
    const distanceFromBottom =
      lastScrollHeight - (lastScrollTop + visibleHeight);

    // Restore the same distance from bottom after resize
    chatContainer.scrollTop =
      newScrollHeight - (distanceFromBottom + visibleHeight);

    // Update values for next resize
    lastScrollHeight = newScrollHeight;
    lastScrollTop = chatContainer.scrollTop;
  }

  // Throttled version to reduce frequency of scroll preservation
  function throttledPreserveScroll() {
    if (resizeThrottleTimeout !== null) {
      return; // Skip if already scheduled
    }

    resizeThrottleTimeout = window.setTimeout(() => {
      preserveScrollFromBottom();
      resizeThrottleTimeout = null;
    }, 100) as unknown as number; // Throttle to max 10 times per second
  }

  function handleScroll() {
    if (!chatContainer) return;

    // Clear existing timeout
    if (scrollTimeout) {
      clearTimeout(scrollTimeout);
    }

    const isScrolledToBottom =
      Math.abs((chatContainer.scrollHeight - chatContainer.scrollTop) - chatContainer.clientHeight) < 10;

    // If user scrolls up, disable auto-scroll
    if (!isScrolledToBottom) {
      autoScroll = false;
    }

    // If user scrolls to bottom, enable auto-scroll after a delay
    if (isScrolledToBottom) {
      scrollTimeout = setTimeout(() => {
        autoScroll = true;
      }, 150); // 150ms delay before re-enabling auto-scroll
    }

    if (
      chatContainer.scrollTop <= LOAD_MORE_THRESHOLD &&
      hasMoreMessages &&
      !loadingMore
    ) {
      loadMoreMessages();
    }
  }

  // Setup event listeners and observers
  function setupScrollBehavior() {
    if (!chatContainer) return;

    // Setup scroll event listener
    chatContainer.addEventListener('scroll', handleScroll);

    // Setup resize observer
    resizeObserver = new ResizeObserver(() => {
      throttledPreserveScroll();
    });
    resizeObserver.observe(chatContainer);
  }

  // Cleanup function
  function cleanup() {
    if (chatContainer) {
      chatContainer.removeEventListener('scroll', handleScroll);
    }
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
    if (scrollTimeout) {
      clearTimeout(scrollTimeout);
      scrollTimeout = null;
    }
    if (scrollThrottleTimeout !== null) {
      clearTimeout(scrollThrottleTimeout);
      scrollThrottleTimeout = null;
    }
    if (resizeThrottleTimeout !== null) {
      clearTimeout(resizeThrottleTimeout);
      resizeThrottleTimeout = null;
    }
  }

  // Initialize on mount
  onMount(() => {
    setupScrollBehavior();
  });

  onDestroy(() => {
    cleanup();
  });

  export function scrollToBottom() {
    if (chatContainer && autoScroll) {
      const newScrollTop = chatContainer.scrollHeight - chatContainer.clientHeight;
      chatContainer.scrollTop = newScrollTop;
      lastScrollHeight = chatContainer.scrollHeight;
      lastScrollTop = newScrollTop;
    }
  }

  // Throttled scroll to bottom to reduce DOM thrashing
  // Use longer throttle during streaming to reduce performance impact
  function throttledScrollToBottom() {
    if (scrollThrottleTimeout !== null) {
      return; // Skip if already scheduled
    }

    scrollThrottleTimeout = window.setTimeout(() => {
      scrollToBottom();
      scrollThrottleTimeout = null;
    }, 100) as unknown as number; // Increased from 16ms to 100ms to reduce frequency
  }

  // Track last streaming message length to avoid triggering on every chunk
  let lastStreamingLength = 0;

  function loadMoreMessages() {
    if (!chatContainer || loadingMore) return;
    loadingMore = true;

    const prevScrollHeight = chatContainer.scrollHeight;
    const prevScrollTop = chatContainer.scrollTop;

    visibleCount = Math.min(messages.length, visibleCount + LOAD_MORE_CHUNK);

    requestAnimationFrame(() => {
      if (!chatContainer) {
        loadingMore = false;
        return;
      }
      const newScrollHeight = chatContainer.scrollHeight;
      chatContainer.scrollTop = newScrollHeight - prevScrollHeight + prevScrollTop;
      lastScrollHeight = chatContainer.scrollHeight;
      lastScrollTop = chatContainer.scrollTop;
      loadingMore = false;
    });
  }

  $: {
    const total = messages.length;
    if (total < lastTotalCount) {
      visibleCount = INITIAL_VISIBLE_MESSAGES;
    } else if (total > lastTotalCount) {
      visibleCount = Math.min(total, Math.max(visibleCount, INITIAL_VISIBLE_MESSAGES));
    }
    lastTotalCount = total;

    const startIndex = Math.max(0, total - visibleCount);
    visibleMessages = messages.slice(startIndex);
    hasMoreMessages = startIndex > 0;
  }

  // Only scroll when messages actually change or streaming updates
  $: if (messages.length !== lastMessageCount || ($streamingMessage && $streamingMessage.length !== lastStreamingLength)) {
    lastMessageCount = messages.length;
    lastStreamingLength = $streamingMessage.length;
    // Use requestAnimationFrame for smoother scrolling
    requestAnimationFrame(() => {
      throttledScrollToBottom();
    });
  }

  // Handle visibility changes - scroll to bottom when page becomes visible
  // This ensures proper scroll position after deferred markdown parsing completes
  $: if ($pageVisible && messages.length > 0) {
    // Scroll immediately for instant feedback
    requestAnimationFrame(() => {
      scrollToBottom();

      // Then wait for deferred content to render and scroll again
      // This catches any height changes from requestIdleCallback parsing in ChatMessage
      if ('requestIdleCallback' in window) {
        requestIdleCallback(() => {
          scrollToBottom();
        });
      } else {
        setTimeout(() => {
          scrollToBottom();
        }, 100);
      }
    });
  }
</script>

<div
  bind:this={chatContainer}
  class="h-full overflow-y-auto pr-4 space-y-4 w-full px-2 md:px-4 lg:px-6"
>
  {#if hasMoreMessages}
    <div class="flex justify-center">
      <button
        class="text-xs text-muted-foreground/80 px-3 py-1 rounded-full border border-white/10 hover:bg-white/5 transition-all"
        onclick={loadMoreMessages}
        disabled={loadingMore}
      >
        {loadingMore ? "Loading..." : "Load earlier messages"}
      </button>
    </div>
  {/if}

  {#each visibleMessages as msg, i (msg.id || `${msg.type}-${i}`)}
    {#if msg.type === "received" && msg.tool_calls && msg.tool_calls.length > 0}
      {#each msg.tool_calls as call (call.execution_id)}
        {#if i >= visibleMessages.length - ANIMATED_MESSAGE_LIMIT}
          <div
            in:fly={{ y: 10, duration: 150, easing: backOut }}
            class="w-full message-container flex justify-start"
          >
            <ToolCallBubble {call} />
          </div>
        {:else}
          <div class="w-full message-container flex justify-start">
            <ToolCallBubble {call} />
          </div>
        {/if}
      {/each}
    {/if}
    {#if shouldRenderMessage(msg)}
      {#if i >= visibleMessages.length - ANIMATED_MESSAGE_LIMIT}
        <div
          in:fly={{ y: 10, duration: 150, easing: backOut }}
          out:scale={{ duration: 100, start: 0.98, opacity: 0 }}
          class="w-full message-container"
        >
          <ChatMessage
            type={msg.type}
            content={msg.content}
            attachments={msg.attachments}
            messageId={msg.id}
            conversationId={conversationId}
            agentActivity={msg.agentActivity}
          />
        </div>
      {:else}
        <div class="w-full message-container">
          <ChatMessage
            type={msg.type}
            content={msg.content}
            attachments={msg.attachments}
            messageId={msg.id}
            conversationId={conversationId}
            agentActivity={msg.agentActivity}
          />
        </div>
      {/if}
    {/if}
  {/each}

  {#if toolApprovals.length > 0}
    <div
      in:fly={{ y: 10, duration: 150, easing: backOut }}
      class="w-full message-container flex justify-start"
    >
      <ToolApprovalQueue approvals={toolApprovals} containerClass="w-full max-w-5xl min-w-0 flex-1" />
    </div>
  {/if}

  {#if showThinkingStatus}
    <div
      in:fly={{ y: 10, duration: 150, easing: backOut }}
      class="w-full message-container flex justify-start"
    >
      <div class="rounded-2xl px-4 py-2 w-full max-w-5xl min-w-0 bg-background/50 border border-border/60">
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <span class="thinking-orb" aria-hidden="true"></span>
          <span>Thinking</span>
          <span class="thinking-dots" aria-hidden="true">
            <span>.</span><span>.</span><span>.</span>
          </span>
          {#if agentPhase !== null && phaseLabel !== "Idle"}
            <span class="text-foreground/80"> {phaseLabel}</span>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- Streaming message displayed separately to avoid array reactivity -->
  {#if $isStreaming}
    <div
      in:fly={{ y: 10, duration: 150, easing: backOut }}
      class="w-full message-container"
    >
      {#if $streamingMessage}
        <ChatMessage
          type="received"
          content={$streamingMessage}
          conversationId={conversationId}
          isStreaming={true}
        />
      {/if}
    </div>
  {/if}
</div>

<style>
  /* Add to existing styles */
  :global(.message-container) {
    transform-origin: center;
    perspective: 1000px;
    transition: all 0.3s ease-out;
  }

  @keyframes dustAway {
    0% {
      opacity: 1;
      transform: translateX(0) rotate(0);
    }
    50% {
      opacity: 0.5;
      transform: translateX(20px) rotate(5deg) scale(0.95);
    }
    100% {
      opacity: 0;
      transform: translateX(40px) rotate(10deg) scale(0.9);
    }
  }

  .thinking-orb {
    width: 6px;
    height: 6px;
    border-radius: 9999px;
    background: rgba(16, 185, 129, 0.75);
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.55);
    animation: orbPulse 1.2s ease-in-out infinite;
  }

  .thinking-dots span {
    display: inline-block;
    margin-left: 2px;
    animation: dotBlink 1.2s ease-in-out infinite;
  }

  .thinking-dots span:nth-child(2) {
    animation-delay: 0.2s;
  }

  .thinking-dots span:nth-child(3) {
    animation-delay: 0.4s;
  }

  @keyframes orbPulse {
    0%,
    100% {
      transform: scale(0.9);
      opacity: 0.6;
    }
    50% {
      transform: scale(1.15);
      opacity: 1;
    }
  }

  @keyframes dotBlink {
    0%,
    100% {
      opacity: 0.2;
    }
    50% {
      opacity: 1;
    }
  }
</style>
