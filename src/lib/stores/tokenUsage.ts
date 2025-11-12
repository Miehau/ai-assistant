import { writable, derived } from 'svelte/store';
import { invoke } from '@tauri-apps/api/tauri';
import type { ConversationUsageSummary } from '$lib/types';
import { formatCost } from '$lib/utils/costCalculator';

// Store for the current conversation's usage summary
export const currentConversationUsage = writable<ConversationUsageSummary | null>(null);

// Derived store for formatted cost
export const formattedCost = derived(
  currentConversationUsage,
  ($usage) => {
    if (!$usage) return '$0.00';
    return formatCost($usage.total_cost);
  }
);

/**
 * Load usage summary for a specific conversation
 * @param conversationId - The conversation ID to load usage for
 */
export async function loadConversationUsage(conversationId: string): Promise<void> {
  try {
    const usage = await invoke<ConversationUsageSummary | null>(
      'get_conversation_usage',
      { conversationId }
    );
    currentConversationUsage.set(usage);
  } catch (error) {
    console.warn('Failed to load conversation usage:', error);
    currentConversationUsage.set(null);
  }
}

/**
 * Clear the current usage data
 */
export function clearConversationUsage(): void {
  currentConversationUsage.set(null);
}
