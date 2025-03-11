<script lang="ts">
  import { onMount, afterUpdate } from "svelte";
  import ChatMessage from "../ChatMessage.svelte";
  import { fade, fly, scale } from "svelte/transition";
  import { backOut } from "svelte/easing";
  import type { Message } from "$lib/types";

  export let messages: Message[] = [];
  export let chatContainer: HTMLElement | null = null;
  export let autoScroll = true;

  let lastScrollHeight = 0;
  let lastScrollTop = 0;
  let scrollTimeout: NodeJS.Timeout | null = null;

  function preserveScrollFromBottom() {
    if (chatContainer) {
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
  }

  // Add resize observer
  onMount(() => {
    if (chatContainer) {
      const resizeObserver = new ResizeObserver(() => {
        preserveScrollFromBottom();
      });

      resizeObserver.observe(chatContainer);

      return () => {
        resizeObserver.disconnect();
      };
    }
  });

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

  // Add the scroll event listener in onMount
  onMount(() => {
    if (chatContainer) {
      chatContainer.addEventListener('scroll', handleScroll);
      return () => {
        chatContainer?.removeEventListener('scroll', handleScroll);
        if (scrollTimeout) clearTimeout(scrollTimeout);
      };
    }
  });

  export function scrollToBottom() {
    if (chatContainer && autoScroll) {
      const newScrollTop = chatContainer.scrollHeight - chatContainer.clientHeight;
      chatContainer.scrollTop = newScrollTop;
      lastScrollHeight = chatContainer.scrollHeight;
      lastScrollTop = newScrollTop;
    }
  }

  // Scroll to bottom when messages change
  afterUpdate(() => {
    scrollToBottom();
  });
</script>

<div
  bind:this={chatContainer}
  class="h-full overflow-y-auto pr-4 space-y-4 w-full"
  on:scroll={handleScroll}
>
  {#each messages as msg, i (i)}
    <div
      in:fly={{ y: 10, duration: 150, delay: i * 10, easing: backOut }}
      out:scale={{ duration: 100, start: 0.98, opacity: 0 }}
      class="w-full message-container"
    >
      <ChatMessage
        type={msg.type}
        content={msg.content}
        attachments={msg.attachments}
      />
    </div>
  {/each}
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
