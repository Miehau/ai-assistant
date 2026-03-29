export const config = {
  apiUrl: import.meta.env.VITE_API_URL || "http://localhost:3000",

  // Model configuration for title generation
  // Using a fast, cost-effective model for this simple task
  titleGeneration: {
    // Preferred model for title generation (cheap via OpenRouter)
    preferredModel: "meta-llama/llama-3.2-3b-instruct:free",
    // Fallback models if preferred is not available
    fallbackModels: ["openai/gpt-4o-mini", "claude-haiku-4-5-20251001"],
  },
};
