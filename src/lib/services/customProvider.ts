import type { Message } from '$lib/types';
import { formatMessages } from './messageFormatting';

export class CustomProviderService {
  async createChatCompletion(
    modelName: string,
    url: string,
    history: any[],
    message: Message,
    systemPrompt: string,
    streamResponse: boolean,
    onStreamResponse: (chunk: string) => void
  ): Promise<string> {
    const body = JSON.stringify({
      messages: formatMessages(history, message, systemPrompt),
      model: modelName,
      stream: streamResponse,
    });

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body,
    });

    if (!response.ok) {
      throw new Error(`Failed to send chat message to custom provider: ${response.statusText}`);
    }

    console.log('response', response.headers.get('content-type'));
    const isStreaming = response.headers.get('content-type')?.includes('text/event-stream') 
      || response.headers.get('content-type')?.includes('application/x-ndjson')
      || response.headers.get('transfer-encoding')?.includes('chunked');

    if (streamResponse && isStreaming && response.body) {
      return this.handleStreamingResponse(response, onStreamResponse);
    }
    console.log('shouldnt be here')
    const data = await response.json();
    const content = data.message?.content || data.choices?.[0]?.message?.content || '';
    if (onStreamResponse) {
      onStreamResponse(content);
    }
    return content;
  }

  private async handleStreamingResponse(
    response: Response, 
    onStreamResponse: (chunk: string) => void
  ): Promise<string> {
    const reader = response.body!.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let fullResponse = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        let newlineIndex;
        while ((newlineIndex = buffer.indexOf('\n')) !== -1) {
          const line = buffer.slice(0, newlineIndex);
          buffer = buffer.slice(newlineIndex + 1);

          if (line.trim()) {
            try {
              const parsed = JSON.parse(line);
              const content = parsed.message?.content || '';
              if (content) {
                fullResponse += content;
                onStreamResponse(content);
              }
            } catch (e) {
              console.error('Error parsing JSON:', e);
            }
          }
        }
      }

      if (buffer.trim()) {
        try {
          const parsed = JSON.parse(buffer);
          const content = parsed.message?.content || '';
          if (content) {
            fullResponse += content;
            onStreamResponse(content);
          }
        } catch (e) {
          console.error('Error parsing final JSON:', e);
        }
      }

      return fullResponse;
    } finally {
      reader.releaseLock();
    }
  }
}

export const customProviderService = new CustomProviderService(); 