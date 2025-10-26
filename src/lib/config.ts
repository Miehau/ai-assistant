export const config = {
  apiUrl: import.meta.env.VITE_API_URL || "http://localhost:3000",

  // Model configuration for title generation
  // Using a fast, cost-effective model for this simple task
  titleGeneration: {
    // Preferred model for title generation (fast and cheap)
    preferredModel: "claude-haiku-4-5-20251001",
    // Fallback models if preferred is not available
    fallbackModels: ["gpt-4o-mini", "gpt-3.5-turbo"],
  },
};
