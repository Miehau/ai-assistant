import { config } from "$lib/config";
import { modelRegistry } from "$lib/models/registry";

/**
 * Service to generate titles for conversations based on their content
 */
export class TitleGeneratorService {
  /**
   * Gets the best available model for title generation based on configuration
   * @returns A promise that resolves to the model name
   */
  private getTitleGenerationModel(): { model: string; provider: string } {
    // Check all registered models (not just those with keys stored) so the
    // Rust backend can produce a proper "missing API key" error if needed,
    // rather than silently falling through to a random available model.
    const allModels = modelRegistry.getAllRegisteredModels();
    const candidates = [
      config.titleGeneration.preferredModel,
      ...config.titleGeneration.fallbackModels,
    ];

    for (const modelId of candidates) {
      const model = allModels[modelId];
      if (model) {
        console.log("Using model for title generation:", modelId);
        return { model: modelId, provider: model.provider };
      }
    }

    throw new Error(
      `No configured title generation models found in registry. Tried: ${candidates.join(", ")}`,
    );
  }

  /**
   * Generates a title for a conversation based on its content
   * @param conversationId The ID of the conversation to generate a title for
   * @returns A promise that resolves to the generated title
   */
  async generateTitle(conversationId: string): Promise<string> {
    // Title generation not yet implemented in server backend
    console.warn('[titleGenerator] Not yet implemented in server backend');
    return "New Conversation";
  }

  /**
   * Generates and updates the title for a conversation
   * @param conversationId The ID of the conversation to update
   */
  async generateAndUpdateTitle(conversationId: string): Promise<void> {
    try {
      console.log(
        "Starting title generation and update for conversation:",
        conversationId,
      );
      await this.generateTitle(conversationId);
      console.log("Title generation completed");
    } catch (error) {
      console.error("Error updating conversation title:", error);
    }
  }
}

export const titleGeneratorService = new TitleGeneratorService();
