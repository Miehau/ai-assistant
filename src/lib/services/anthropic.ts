import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';

export class AnthropicService {
  constructor(private apiKey: string) {}

  async createChatCompletion(
    model: string,
    messages: ChatCompletionMessageParam[],
    stream: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal
  ): Promise<string> {
    const systemMessage = messages.find(m => m.role === 'system');
    const userMessages = messages.filter(m => m.role !== 'system');
    
    // Format messages for Anthropic's API
    const formattedMessages = userMessages.map(msg => ({
      role: msg.role === 'assistant' ? 'assistant' : 'user',
      content: msg.content
    }));
    const body = {
      model,
      messages: formattedMessages,
      system: systemMessage?.content,
      stream,
      max_tokens: 4096
    };

    const response = await fetch('https://api.anthropic.com/v1/messages', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'x-api-key': this.apiKey,
        'anthropic-version': '2023-06-01',
        "anthropic-dangerous-direct-browser-access": "true",
      },
      body: JSON.stringify(body),
      signal
    });

    if (!response.ok) {
        const data = await response.json();
      throw new Error(`Anthropic API error: ${JSON.stringify(data)}`);
    }

    if (stream) {
      const reader = response.body?.getReader();
      const decoder = new TextDecoder();
      let fullText = '';
      
      if (!reader) throw new Error('No reader available');

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value);
        const lines = chunk.split('\n').filter(line => line.trim());
        
        for (const line of lines) {
          if (line.startsWith('data: ')) {
            const data = line.slice(6);
            if (data === '[DONE]') continue;
            
            try {
              const parsed = JSON.parse(data);
              if (parsed.type === 'content_block_start' || parsed.type === 'content_block_delta') {
                const text = parsed.delta?.text || '';
                fullText += text;
                onStreamResponse(text);
              }
            } catch (e) {
              console.error('Error parsing stream:', e);
            }
          }
        }
      }
      
      return fullText;
    } else {
      const data = await response.json();
      onStreamResponse(data.content[0].text);
      return data.content[0].text;
    }
  }
} 