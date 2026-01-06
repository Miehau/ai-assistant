import { config } from "$lib/config";
import { modelRegistry } from "$lib/models/registry";
import { invoke } from "@tauri-apps/api/tauri";

/**
 * Service to generate titles for conversations based on their content
 */
export class TitleGeneratorService {
  /**
   * Gets the best available model for title generation based on configuration
   * @returns A promise that resolves to the model name
   */
  private getTitleGenerationModel(): { model: string; provider: string } {
    try {
      const models = modelRegistry.getAllModels();

      // Try preferred model first
      const preferredModel = models[config.titleGeneration.preferredModel];
      if (preferredModel) {
        console.log(
          "Using preferred model for title generation:",
          config.titleGeneration.preferredModel,
        );
        return {
          model: config.titleGeneration.preferredModel,
          provider: preferredModel.provider,
        };
      }

      // Try fallback models
      for (const fallbackModelName of config.titleGeneration.fallbackModels) {
        const fallbackModel = models[fallbackModelName];
        if (fallbackModel) {
          console.log(
            "Using fallback model for title generation:",
            fallbackModelName,
          );
          return {
            model: fallbackModelName,
            provider: fallbackModel.provider,
          };
        }
      }

      // Last resort: use any available model with text capability
      const firstTextModel = Object.entries(models).find(
        ([_, model]) => model.capabilities.text,
      );
      if (firstTextModel) {
        const [modelId, model] = firstTextModel;
        console.log(
          "Using first available model for title generation:",
          model.name,
        );
        return {
          model: modelId,
          provider: model.provider,
        };
      }

      throw new Error("No enabled models found for title generation");
    } catch (error) {
      console.error("Failed to get title generation model:", error);
      throw error;
    }
  }

  /**
   * Generates a title for a conversation based on its content
   * @param conversationId The ID of the conversation to generate a title for
   * @returns A promise that resolves to the generated title
   */
  async generateTitle(conversationId: string): Promise<string> {
    try {
      console.log("Generating title for conversation:", conversationId);

      const selectedModel = this.getTitleGenerationModel();
      console.log("Using model for title generation:", selectedModel.model);

      const response = await invoke<{ title: string }>("agent_generate_title", {
        payload: {
          conversation_id: conversationId,
          model: selectedModel.model,
          provider: selectedModel.provider,
        },
      });

      const title = response.title.trim().replace(/^["']|["']$/g, "");

      // Ensure the title is not too long
      const finalTitle =
        title.length > 50 ? title.substring(0, 47) + "..." : title;
      console.log("Generated final title:", finalTitle);

      return finalTitle;
    } catch (error) {
      console.error("Error generating title:", error);
      return "New Conversation";
    }
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
