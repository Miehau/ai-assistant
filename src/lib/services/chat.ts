import { invoke } from '@tauri-apps/api/tauri';
import OpenAI from 'openai';
import { HumanMessage, SystemMessage, AIMessage } from '@langchain/core/messages';

async function getApiKeyForProvider(provider: string): Promise<string> {
  const apiKey = await invoke<string | null>('get_api_key', { provider });
  if (!apiKey) {
    throw new Error(`No API key found for provider: ${provider}`);
  }
  return apiKey;
}

export async function sendChatMessage(
  message: string,
  conversationId: string | null,
  model: string,
  streamResponse: boolean,
  onStream?: (chunk: string) => void
) {
  console.log("Using model:", model);
  try {
    // Get or create conversation
    const conversation: { id: string } = await invoke('get_or_create_conversation', { conversationId });
    
    // Get conversation history
    const history: { role: string, content: string }[] = await invoke('get_conversation_history', { conversationId: conversation.id });

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
        content: "You will act as a Senior Software Recruitment Specialist. You will be given candidate's skills together with job description. You task is to select most relevant skills from candidate's skills that will match the job description, increasing candidate's chances of getting the job. You will output them in JSON object containing fields: 'company', 'roleName', 'keyPoints', the last one being array of strings with most relevant experience for the description. \n\n"
      },
      ...history.map((msg) => ({
        role: msg.role,
        content: msg.content
      })),
      { role: 'user', content: message }
    ];

    // Call OpenAI API with streaming
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
