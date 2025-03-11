import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';

export class DeepSeekService {
  constructor(private apiKey: string) {}

  async createChatCompletion(
    model: string,
    messages: ChatCompletionMessageParam[],
    streamResponse: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal
  ): Promise<string> {
    const response = await fetch("https://api.deepseek.com/chat/completions", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Access-Control-Allow-Origin": "*",
        Authorization: `Bearer ${this.apiKey}`,

      },
      body: JSON.stringify({
        model,
        messages,
        stream: streamResponse,
      }),
      signal,
    });

    if (!response.ok) {
      throw new Error(`DeepSeek API error: ${response.statusText}`);
    }

    if (!streamResponse) {
      const data = await response.json();
      const content = data.choices[0]?.message?.content || '';
      onStreamResponse(content);
      return content;
    }

    return this.handleStreamingResponse(response, onStreamResponse);
  }

  private async handleStreamingResponse(
    response: Response, 
    onStreamResponse: (chunk: string) => void
  ): Promise<string> {
    const reader = response.body?.getReader();
    if (!reader) throw new Error('No response body reader available');
    
    let fullResponse = '';
    const decoder = new TextDecoder();

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const lines = decoder.decode(value).split('\n');
        const chunks = lines
          .map(line => line.replace(/^data: /, '').trim())
          .filter(line => line && line !== '[DONE]')
          .map(line => {
            try {
              return JSON.parse(line);
            } catch {
              return null;
            }
          })
          .filter(data => data?.choices?.[0]?.delta?.content)
          .map(data => data.choices[0].delta.content);

        for (const chunk of chunks) {
          fullResponse += chunk;
          onStreamResponse(chunk);
        }
      }
      return fullResponse;
    } finally {
      reader.releaseLock();
    }
  }

  async transcribeAudio(base64Audio: string, context: string): Promise<string> {
    throw new Error(`DeepSeek API error: Not implemented`);
  }
}