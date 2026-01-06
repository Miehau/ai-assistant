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
    systemPrompt?: string;
  }

  let { conversationId, modelId = '', currentMessage = '', messages = [], isLoading = false, systemPrompt = '' }: Props = $props();

  // Load usage data when conversation changes
  $effect(() => {
    if (conversationId) {
      loadConversationUsage(conversationId);
    }
  });

  // Derived reactive values
  const usage = $derived($currentConversationUsage);
  const cost = $derived($formattedCost);

  // Refresh usage when messages change or streaming ends
  let usageRefreshTimeout: number | null = null;
  let lastMessageCount = $state(0);
  let lastIsLoading = $state(false);

  $effect(() => {
    if (!conversationId) return;

    const messagesChanged = messages.length !== lastMessageCount;
    const loadingEnded = lastIsLoading && !isLoading;
    lastIsLoading = isLoading;

    if (!messagesChanged && !loadingEnded) return;

    lastMessageCount = messages.length;

    if (usageRefreshTimeout !== null) {
      clearTimeout(usageRefreshTimeout);
    }

    const delay = isLoading ? 600 : 200;
    usageRefreshTimeout = window.setTimeout(() => {
      loadConversationUsage(conversationId);
      usageRefreshTimeout = null;
    }, delay) as unknown as number;
  });

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

    // When no conversation exists, include system prompt in the estimate
    const systemPromptEstimate = !usage && systemPrompt ? estimateTokens(systemPrompt) : 0;

    const total = baseTokens + typingEstimate + streamingEstimate + systemPromptEstimate;
    const isEstimating = !usage || typingEstimate > 0 || streamingEstimate > 0;

    return { total, isEstimating };
  });

  // Get max context window for the model
  const maxTokens = $derived(modelId ? getModelContextWindow(modelId) : 128000);
</script>

<div class="glass-badge-subtle px-2.5 py-1.5 rounded-lg flex items-center gap-2 text-[11px] text-muted-foreground/70 border-white/10">
  <div class="flex flex-col leading-tight">
    <span class="text-[9px] uppercase tracking-wide text-muted-foreground/50">Tokens</span>
    <span class="font-mono text-foreground/80">
      {displayTokens().isEstimating ? '~' : ''}{displayTokens().total.toLocaleString()}
      <span class="text-muted-foreground/40 mx-0.5">/</span>{maxTokens.toLocaleString()}
    </span>
  </div>
  <span class="h-6 w-px bg-white/10"></span>
  <div class="flex flex-col leading-tight">
    <span class="text-[9px] uppercase tracking-wide text-muted-foreground/50">Cost</span>
    <span class="font-mono text-foreground/80">{cost}</span>
  </div>
</div>
