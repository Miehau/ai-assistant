import { invoke } from '@tauri-apps/api/tauri';
import OpenAI from 'openai';
import { HumanMessage, SystemMessage, AIMessage } from '@langchain/core/messages';

// Initialize OpenAI client
const openai = new OpenAI({
  apiKey: "key", // Make sure to set this environment variable
  dangerouslyAllowBrowser: true // This is needed for client-side usage
});

export async function sendChatMessage(
  message: string,
  conversationId: string | null,
  model: string,
  streamResponse: boolean,
  onStream?: (chunk: string) => void
) {
  try {
    // Get or create conversation
    const conversation: { id: string } = await invoke('get_or_create_conversation', { conversationId });
    
    // Get conversation history
    const history: { role: string, content: string }[] = await invoke('get_conversation_history', { conversationId: conversation.id });

    // Prepare messages for OpenAI
    const messages: ChatCompletionMessageParam[] = [
      { role: 'system', content: "You are a helpful assistant." },
      ...history.map((msg) => ({
        role: msg.role as ChatCompletionRequestMessageRoleEnum,
        content: msg.content
      })),
      { role: 'user', content: message }
    ];

    // Call OpenAI API with streaming
    const stream = await openai.chat.completions.create({
      model: model || 'gpt-3.5-turbo',
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

    // Save messages
    await invoke('save_message', { 
      conversationId: conversation.id, 
      role: 'user', 
      content: message 
    });
    await invoke('save_message', { 
      conversationId: conversation.id, 
      role: 'assistant', 
      content: fullResponse 
    });

    return {
      text: fullResponse,
      conversationId: conversation.id,
    };
  } catch (error) {
    console.error('Failed to send chat message:', error);
    throw new Error('Failed to send chat message');
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

