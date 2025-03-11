import { invoke } from '@tauri-apps/api/tauri';
import type { APIMessage } from '$lib/types';
import { conversationService } from './conversation';
import { chatService } from './chat';

/**
 * Service to generate titles for conversations based on their content
 */
export class TitleGeneratorService {
  /**
   * Generates a title for a conversation based on its content
   * @param conversationId The ID of the conversation to generate a title for
   * @returns A promise that resolves to the generated title
   */
  async generateTitle(conversationId: string): Promise<string> {
    try {
      console.log('Generating title for conversation:', conversationId);
      
      // Get the conversation history
      const messages = await conversationService.getAPIHistory(conversationId);
      console.log('Retrieved messages for title generation:', messages.length);
      
      // If there are no messages, return a default title
      if (!messages || messages.length === 0) {
        console.log('No messages found, using default title');
        return "New Conversation";
      }
      
      // Extract user messages for context
      const userMessages = messages.filter(msg => msg.role === 'user');
      console.log('Found user messages:', userMessages.length);
      
      // If there are no user messages, return a default title
      if (userMessages.length === 0) {
        console.log('No user messages found, using default title');
        return "New Conversation";
      }
      
      // Create a prompt for the title generation
      const titlePrompt: APIMessage[] = [
        {
          role: 'system',
          content: 'You are a helpful assistant that generates short, descriptive titles for conversations. ' +
                  'Generate a concise title (maximum 5 words) that captures the main topic or intent of the conversation. ' +
                  'Respond ONLY with the title, no quotes, no explanation, no punctuation at the end.'
        }
      ];
      
      // Add the first user message as context
      titlePrompt.push({
        role: 'user',
        content: `Generate a short title for a conversation that starts with this message: "${userMessages[0].content}"`
      });
      
      console.log('Title generation prompt:', titlePrompt);
      
      // Use the chat service to generate a title
      const model = await chatService.getDefaultModel();
      console.log('Using model for title generation:', model);
      
      const titleResponse = await chatService.generateCompletion(titlePrompt, model);
      console.log('Raw title response:', titleResponse);
      
      // Clean up the response
      const title = titleResponse.trim().replace(/^["']|["']$/g, '');
      
      // Ensure the title is not too long
      const finalTitle = title.length > 50 ? title.substring(0, 47) + '...' : title;
      console.log('Generated final title:', finalTitle);
      
      return finalTitle;
    } catch (error) {
      console.error('Error generating title:', error);
      return "New Conversation";
    }
  }
  
  /**
   * Generates and updates the title for a conversation
   * @param conversationId The ID of the conversation to update
   */
  async generateAndUpdateTitle(conversationId: string): Promise<void> {
    try {
      console.log('Starting title generation and update for conversation:', conversationId);
      const title = await this.generateTitle(conversationId);
      console.log('About to update conversation name to:', title);
      await conversationService.updateConversationName(conversationId, title);
      console.log('Successfully updated conversation name');
    } catch (error) {
      console.error('Error updating conversation title:', error);
    }
  }
}

export const titleGeneratorService = new TitleGeneratorService();
