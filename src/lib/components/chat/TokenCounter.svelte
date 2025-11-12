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

  // Calculate live token estimation
  const displayTokens = $derived(() => {
    const baseTokens = usage?.total_tokens || 0;

    // Estimate tokens from current message being typed
    const typingEstimate = currentMessage ? estimateTokens(currentMessage) : 0;

    // Estimate tokens from streaming AI response
    let streamingEstimate = 0;
    if (isLoading && messages.length > 0) {
      const lastMessage = messages[messages.length - 1];
      if (lastMessage.type === 'received') {
        streamingEstimate = estimateTokens(lastMessage.content);
      }
    }

    const total = baseTokens + typingEstimate + streamingEstimate;
    const isEstimating = typingEstimate > 0 || streamingEstimate > 0;

    return { total, isEstimating };
  });

  // Get max context window for the model
  const maxTokens = $derived(modelId ? getModelContextWindow(modelId) : 128000);
</script>

<div class="glass-badge px-3 py-1.5 rounded-lg flex items-center gap-2 text-sm">
  {#if usage}
    <div class="flex items-center gap-1">
      <span class="text-muted-foreground">Tokens:</span>
      <span class="font-medium">
        {displayTokens().isEstimating ? '~' : ''}{displayTokens().total.toLocaleString()} / {maxTokens.toLocaleString()}
      </span>
    </div>
    <div class="h-3 w-px bg-border"></div>
    <div class="flex items-center gap-1">
      <span class="text-muted-foreground">Cost:</span>
      <span class="font-medium">{cost}</span>
    </div>
  {:else}
    <span class="text-muted-foreground">No usage data</span>
  {/if}
</div>
