<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import ChatMessage from "../ChatMessage.svelte";
  import { fade, fly, scale } from "svelte/transition";
  import { backOut } from "svelte/easing";
  import type { Message } from "$lib/types";
  import { streamingMessage, isStreaming } from "./store";

  export let messages: Message[] = [];
  export let chatContainer: HTMLElement | null = null;
  export let autoScroll = true;
  export let conversationId: string | undefined = undefined;

  let lastScrollHeight = 0;
  let lastScrollTop = 0;
  let scrollTimeout: NodeJS.Timeout | null = null;
  let lastMessageCount = 0;
  let scrollThrottleTimeout: number | null = null;
  let resizeThrottleTimeout: number | null = null;
  let resizeObserver: ResizeObserver | null = null;

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

  // Only scroll when messages actually change or streaming updates significantly
  $: if (messages.length !== lastMessageCount || ($streamingMessage && $streamingMessage.length - lastStreamingLength > 50)) {
    lastMessageCount = messages.length;
    lastStreamingLength = $streamingMessage.length;
    // Use requestAnimationFrame for smoother scrolling
    requestAnimationFrame(() => {
      throttledScrollToBottom();
    });
  }
</script>

<div
  bind:this={chatContainer}
  class="h-full overflow-y-auto pr-4 space-y-4 w-full"
  on:scroll={handleScroll}
>
  {#each messages as msg, i (msg.id || `${msg.type}-${i}`)}
    <div
      in:fly={{ y: 10, duration: 150, delay: i * 10, easing: backOut }}
      out:scale={{ duration: 100, start: 0.98, opacity: 0 }}
      class="w-full message-container"
    >
      <ChatMessage
        type={msg.type}
        content={msg.content}
        attachments={msg.attachments}
        messageId={msg.id}
        conversationId={conversationId}
      />
    </div>
  {/each}

  <!-- Streaming message displayed separately to avoid array reactivity -->
  {#if $isStreaming && $streamingMessage}
    <div
      in:fly={{ y: 10, duration: 150, easing: backOut }}
      class="w-full message-container"
    >
      <ChatMessage
        type="received"
        content={$streamingMessage}
        conversationId={conversationId}
      />
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
</style>
