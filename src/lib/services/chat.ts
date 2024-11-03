import { invoke } from '@tauri-apps/api/tauri';
import OpenAI from 'openai';
import { HumanMessage, SystemMessage, AIMessage } from '@langchain/core/messages';
import type { Message } from '$lib/types';

async function getApiKeyForProvider(provider: string): Promise<string> {
  const apiKey = await invoke<string | null>('get_api_key', { provider });
  if (!apiKey) {
    throw new Error(`No API key found for provider: ${provider}`);
  }
  return apiKey;
}

// Add new interface for attachment
interface Attachment {
  type: string;
  base64Data: string;
}

export async function sendChatMessage(
  message: Message,
  conversationId: string | null,
  model: string,
  streamResponse: boolean,
  onStream?: (chunk: string) => void,
  systemPrompt?: string,
  attachment?: Attachment
) {
  console.log("Using model:", model);
  try {
    // Get or create conversation
    const conversation: { id: string } = await invoke('get_or_create_conversation', { conversationId });
    
    // Get conversation history
    const history: { role: string, content: string, attachments: Attachment[] }[] = await invoke('get_conversation_history', { conversationId: conversation.id });

    console.log('history', history);
    // Get all models to find the provider for the selected model
    const models = await invoke<Array<{ model_name: string, provider: string }>>('get_models');
    const selectedModel = models.find(m => m.model_name === model);
    
    if (!selectedModel) {
      throw new Error(`Model ${model} not found`);
    }

    // Get the API key for the provider
    const apiKey = await getApiKeyForProvider(selectedModel.provider);

    // Initialize OpenAI client with the fetched API key
    const openai = new OpenAI({
      apiKey: apiKey,
      dangerouslyAllowBrowser: true
    });

    // Prepare messages for OpenAI
    const messages = [
      { 
        role: 'system', 
        content: systemPrompt || "You are a helpful AI assistant."
      },
      ...history.map((msg) => ({
        role: msg.role,
        content: msg.attachments ? [
          { type: "text", text: msg.content },
          ...msg.attachments.map((att) => ({
            type: "image_url",
            image_url: {
              url: `${att.file_path}`,
              detail: "auto"
            }
          }))
        ] : msg.content
      })),
      { 
        role: 'user', 
        content: message.attachments ? [
          { type: "text", text: message.content },
          ...message.attachments.map((att) => ({
            type: "image_url",
            image_url: {
              url: `${att.data}`,
              detail: "auto"
            }
          }))
        ] : message.content
      }
    ];
    console.log('message ', messages);

    const stream = await openai.chat.completions.create({
      model: model,
      messages: messages,
      stream: streamResponse,
    });

    let fullResponse = '';
    for await (const chunk of stream) {
      const content = chunk.choices[0]?.delta?.content || '';
      fullResponse += content;
      if (onStream) {
        onStream(content);
      }
    }


    console.log('saving message', message);
    console.log('full response', fullResponse);
    // Save messages
    await invoke('save_message', { 
      conversationId: conversation.id, 
      role: 'user', 
      content: message.content,
      attachments: message.attachments || []
    });
    await invoke('save_message', { 
      conversationId: conversation.id, 
      role: 'assistant', 
      content: fullResponse,
      attachments: []
    });
    return {
      text: fullResponse,
      conversationId: conversation.id,
    };
  } catch (error) {
    console.error('Failed to send chat message:', error);
    throw error; // Re-throw to handle in the UI
  }
}

export async function getConversationHistory(conversationId: string) {
  try {
    const history: { role: string, content: string }[] = await invoke('get_conversation_history', { conversationId });
    return history;
  } catch (error) {
    console.error('Failed to get conversation history:', error);
    throw new Error('Failed to get conversation history');
  }
}

export async function getConversations() {
  try {
    const conversations: { id: string, name: string }[] = await invoke('get_conversations');
    return conversations;
  } catch (error) {
    console.error('Failed to get conversations:', error);
    throw new Error('Failed to get conversations');
  }
}
