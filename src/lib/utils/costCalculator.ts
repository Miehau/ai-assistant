import pricingData from '$lib/models/registry/pricing.json';
import modelsData from '$lib/models/registry/models.json';

interface PricingInfo {
  input: number;
  output: number;
  per: number;
  currency: string;
  note?: string;
}

interface PricingData {
  pricing: {
    [key: string]: PricingInfo | any; // Allow for non-standard pricing structures like DALL-E
  };
}

interface ModelData {
  models: Array<{
    id: string;
    name: string;
    specs: {
      contextWindow: number;
      tokenization?: {
        averageTokensPerChar?: number;
      };
    };
  }>;
}

const pricing = pricingData as PricingData;
const models = modelsData as ModelData;

/**
 * Calculate the cost for a given model based on token usage
 * @param modelId - The model identifier (e.g., 'gpt-4o', 'claude-sonnet-4-5-20250929')
 * @param promptTokens - Number of input/prompt tokens
 * @param completionTokens - Number of output/completion tokens
 * @returns The estimated cost in USD
 */
export function calculateCost(
  modelId: string,
  promptTokens: number,
  completionTokens: number
): number {
  const normalizedModelId = modelId.replace('claude-cli-', 'claude-');

  // Get pricing for the model
  const modelPricing = pricing.pricing[normalizedModelId];

  if (!modelPricing) {
    console.warn(`No pricing information found for model: ${modelId}`);
    return 0;
  }

  // Calculate cost based on pricing per tokens (usually per 1000 tokens)
  const inputCost = (promptTokens / modelPricing.per) * modelPricing.input;
  const outputCost = (completionTokens / modelPricing.per) * modelPricing.output;

  const totalCost = inputCost + outputCost;

  // Round to 6 decimal places to avoid floating point issues
  return Math.round(totalCost * 1000000) / 1000000;
}

/**
 * Estimate the number of tokens in a text string
 * This is a rough estimation - actual tokenization varies by model
 * @param text - The text to estimate tokens for
 * @param averagePerChar - Average tokens per character (default: 0.25 which is ~4 chars per token)
 * @returns Estimated number of tokens
 */
export function estimateTokens(text: string, averagePerChar: number = 0.25): number {
  if (!text) return 0;

  // Simple estimation: count characters and apply average
  // For better accuracy, you might want to:
  // - Count words and multiply by 1.3 (rough average)
  // - Use a tokenizer library
  // - Account for special characters, code, etc.

  const charCount = text.length;
  const estimatedTokens = Math.ceil(charCount * averagePerChar);

  return estimatedTokens;
}

/**
 * Format cost as a currency string
 * @param cost - The cost in USD
 * @returns Formatted currency string
 */
export function formatCost(cost: number): string {
  if (cost === 0) return '$0.00';

  // For very small amounts, show more precision
  if (cost < 0.01) {
    return `$${cost.toFixed(6)}`;
  }

  return `$${cost.toFixed(2)}`;
}

/**
 * Get pricing information for a model
 * @param modelId - The model identifier
 * @returns Pricing info or null if not found
 */
export function getModelPricing(modelId: string): PricingInfo | null {
  const normalizedModelId = modelId.replace('claude-cli-', 'claude-');
  return pricing.pricing[normalizedModelId] || null;
}

/**
 * Get the context window size for a model
 * @param modelId - The model identifier
 * @returns Context window size in tokens, or 128000 as default
 */
export function getModelContextWindow(modelId: string): number {
  const normalizedModelId = modelId.replace('claude-cli-', 'claude-');
  const model = models.models.find(m => m.id === normalizedModelId)
    ?? models.models.find(m => m.id === modelId);
  return model?.specs.contextWindow || 128000;
}

/**
 * Format a token count as a compact string (e.g., 128000 -> "128K", 1000000 -> "1M")
 * @param tokens - The token count to format
 * @returns Formatted string
 */
export function formatTokenCount(tokens: number): string {
  if (tokens >= 1000000) {
    return `${(tokens / 1000000).toFixed(1)}M`.replace('.0M', 'M');
  }
  if (tokens >= 1000) {
    return `${(tokens / 1000).toFixed(1)}K`.replace('.0K', 'K');
  }
  return tokens.toString();
}
