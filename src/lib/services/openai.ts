import OpenAI from 'openai';
import type { Message } from '$lib/types';

interface OpenAIMessage {
  role: 'system' | 'user' | 'assistant';
  content: string | Array<{ type: string; text?: string; image_url?: { url: string; detail: string } }>;
}

export class OpenAIService {
  private client: OpenAI;

  constructor(apiKey: string) {
    this.client = new OpenAI({
      apiKey,
      dangerouslyAllowBrowser: true
    });
  }

  private formatMessages(history: any[], currentMessage: Message, systemPrompt?: string): OpenAIMessage[] {
    return [
      { 
        role: 'system', 
        content: systemPrompt || "You are a helpful AI assistant."
      },
      ...history.map(this.formatHistoryMessage),
      this.formatUserMessage(currentMessage)
    ];
  }

  private formatHistoryMessage(msg: any): OpenAIMessage {
    return {
      role: msg.role,
      content: msg.attachments ? [
        { type: "text", text: msg.content },
        ...msg.attachments.map((att: any) => ({
          type: "image_url",
          image_url: {
            url: `${att.file_path}`,
            detail: "auto"
          }
        }))
      ] : msg.content
    };
  }

  private formatUserMessage(message: Message): OpenAIMessage {
    return {
      role: 'user',
      content: message.attachments ? [
        { type: "text", text: message.content },
        ...message.attachments.map(att => ({
          type: "image_url",
          image_url: {
            url: `${att.data}`,
            detail: "auto"
          }
        }))
      ] : message.content
    };
  }

  async createChatCompletion(
    model: string,
    history: any[],
    message: Message,
    systemPrompt?: string,
    streamResponse = false,
    onStream?: (chunk: string) => void,
  ) {
    const messages = this.formatMessages(history, message, systemPrompt);
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