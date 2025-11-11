<script lang="ts">
  import { estimateTokens, calculateCost, formatCost, getModelContextWindow, formatTokenCount } from '$lib/utils/costCalculator';
  import type { Message, Attachment } from '$lib/types';

  interface Props {
    modelId: string;
    messageText: string;
    messages?: Message[];
    systemPrompt?: string;
    attachments?: Attachment[];
  }

  let { modelId, messageText, messages = [], systemPrompt = '', attachments = [] }: Props = $props();

  // Reactive state for estimated values
  let totalTokens = $state(0);
  let maxTokens = $state(128000);
  let estimatedCost = $state(0);
  let formattedEstimatedCost = $state('$0.00');
  let formattedTotal = $state('0');
  let formattedMax = $state('128K');

  // Update estimates when inputs change
  $effect(() => {
    if (!modelId) {
      totalTokens = 0;
      maxTokens = 128000;
      estimatedCost = 0;
      formattedEstimatedCost = '$0.00';
      formattedTotal = '0';
      formattedMax = '128K';
      return;
    }

    // Get context window for the model
    maxTokens = getModelContextWindow(modelId);
    formattedMax = formatTokenCount(maxTokens);

    // Calculate total input tokens
    let count = 0;

    // 1. Current message being typed
    if (messageText) {
      count += estimateTokens(messageText);
    }

    // 2. System prompt
    if (systemPrompt) {
      count += estimateTokens(systemPrompt);
    }

    // 3. Conversation history
    if (messages) {
      messages.forEach(msg => {
        if (msg.content) {
          count += estimateTokens(msg.content);
        }
      });
    }

    // 4. Text attachments (estimate based on data)
    if (attachments) {
      attachments.forEach(att => {
        if (att.attachment_type === 'text' && att.data) {
          count += estimateTokens(att.data);
        }
      });
    }

    totalTokens = count;
    formattedTotal = formatTokenCount(count);

    // Estimate completion tokens (rough estimate: same as prompt)
    const estimatedCompletionTokens = Math.min(count, maxTokens / 2);

    // Calculate estimated cost
    estimatedCost = calculateCost(modelId, count, estimatedCompletionTokens);
    formattedEstimatedCost = formatCost(estimatedCost);
  });
</script>

<!-- Always render to prevent layout shift -->
<div class="glass-badge px-2 py-0.5 rounded text-xs flex items-center gap-1.5 opacity-85 min-w-[140px]">
  <span class="text-muted-foreground whitespace-nowrap">{formattedTotal} / {formattedMax}</span>
  <span class="text-muted-foreground">•</span>
  <span class="text-muted-foreground whitespace-nowrap">{formattedEstimatedCost}</span>
</div>
