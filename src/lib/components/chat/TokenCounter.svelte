<script lang="ts">
  import { currentConversationUsage, formattedCost, loadConversationUsage } from '$lib/stores/tokenUsage';
  import { getModelContextWindow, estimateTokens } from '$lib/utils/costCalculator';
  import type { Message } from '$lib/types';

  interface Props {
    conversationId?: string;
    modelId?: string;
    currentMessage?: string;
    messages?: Message[];
    isLoading?: boolean;
  }

  let { conversationId, modelId = '', currentMessage = '', messages = [], isLoading = false }: Props = $props();

  // Load usage data when conversation changes
  $effect(() => {
    if (conversationId) {
      loadConversationUsage(conversationId);
    }
  });

  // Derived reactive values
  const usage = $derived($currentConversationUsage);
  const cost = $derived($formattedCost);

  // Debounced values for token estimation
  let debouncedCurrentMessage = $state('');
  let debouncedStreamingContent = $state('');
  let typingDebounceTimeout: number | null = null;
  let streamingDebounceTimeout: number | null = null;

  // Debounce current message changes (300ms)
  $effect(() => {
    if (typingDebounceTimeout !== null) {
      clearTimeout(typingDebounceTimeout);
    }
    typingDebounceTimeout = window.setTimeout(() => {
      debouncedCurrentMessage = currentMessage;
      typingDebounceTimeout = null;
    }, 300) as unknown as number;
  });

  // Debounce streaming content changes (200ms)
  $effect(() => {
    if (isLoading && messages.length > 0) {
      const lastMessage = messages[messages.length - 1];
      if (lastMessage.type === 'received') {
        if (streamingDebounceTimeout !== null) {
          clearTimeout(streamingDebounceTimeout);
        }
        streamingDebounceTimeout = window.setTimeout(() => {
          debouncedStreamingContent = lastMessage.content;
          streamingDebounceTimeout = null;
        }, 200) as unknown as number;
      }
    } else {
      debouncedStreamingContent = '';
    }
  });

  // Calculate live token estimation using debounced values
  const displayTokens = $derived(() => {
    const baseTokens = usage?.total_tokens || 0;

    // Estimate tokens from current message being typed (debounced)
    const typingEstimate = debouncedCurrentMessage ? estimateTokens(debouncedCurrentMessage) : 0;

    // Estimate tokens from streaming AI response (debounced)
    const streamingEstimate = debouncedStreamingContent ? estimateTokens(debouncedStreamingContent) : 0;

    const total = baseTokens + typingEstimate + streamingEstimate;
    const isEstimating = typingEstimate > 0 || streamingEstimate > 0;

    return { total, isEstimating };
  });

  // Get max context window for the model
  const maxTokens = $derived(modelId ? getModelContextWindow(modelId) : 128000);
</script>

<div class="flex items-center gap-1.5 text-[10px] text-muted-foreground/50 hover:text-muted-foreground/70 transition-colors duration-200">
  {#if usage}
    <div class="flex items-center gap-1">
      <span>Tokens:</span>
      <span class="font-mono text-foreground/60">
        {displayTokens().isEstimating ? '~' : ''}{displayTokens().total.toLocaleString()}<span class="text-muted-foreground/40 mx-0.5">/</span>{maxTokens.toLocaleString()}
      </span>
    </div>
  {:else}
    <span>No usage data</span>
  {/if}
</div>
