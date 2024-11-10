import OpenAI from 'openai';
import type { Message } from '$lib/types';
import { formatMessages } from './messageFormatting';

export class OpenAIService {
  private client: OpenAI;

  constructor(apiKey: string) {
    this.client = new OpenAI({
      apiKey,
      dangerouslyAllowBrowser: true
    });
  }

  async createChatCompletion(
    model: string,
    history: any[],
    message: Message,
    systemPrompt?: string,
    streamResponse = false,
    onStream?: (chunk: string) => void,
  ) {
    const messages = formatMessages(history, message, systemPrompt);
    console.log(messages);
    
    const stream = await this.client.chat.completions.create({
      model,
      messages,
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

    return fullResponse;
  }
}