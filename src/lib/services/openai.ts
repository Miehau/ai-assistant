import OpenAI from 'openai';
import type { Message } from '$lib/types';
import { formatMessages } from './messageFormatting';
import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';

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
    const messages = formatMessages(history, message, systemPrompt) as ChatCompletionMessageParam[];
    
    try {
        if (streamResponse) {
            const stream = await this.client.chat.completions.create({
                model,
                messages,
                stream: true,
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
    } catch (error) {
        // If streaming fails, fall back to non-streaming
        console.warn('Streaming not supported, falling back to regular response');
    }

    // Default to non-streaming response
    const response = await this.client.chat.completions.create({
        model,
        messages,
        stream: false,
    });
    const content = response.choices[0]?.message?.content || '';
    if (onStream) {
        onStream(content);
    }
    return content;
  }
}