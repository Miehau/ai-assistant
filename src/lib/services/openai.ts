import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';
import { ChatOpenAI } from '@langchain/openai';
import type { Attachment } from '$lib/types';

export class OpenAIService {
  constructor(private apiKey: string) {}

  async createChatCompletion(
    model: string,
    messages: ChatCompletionMessageParam[],
    streamResponse: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal
  ): Promise<{ content: string; usage?: { prompt_tokens: number; completion_tokens: number } }> {
    const timeoutDuration = 300000; // 5 minutes timeout
    const timeoutController = new AbortController();
    const timeoutId = setTimeout(() => timeoutController.abort('Request timed out'), timeoutDuration);

    // Combine timeout signal with the provided signal
    const combinedController = new AbortController();
    signal.addEventListener('abort', () => combinedController.abort());
    timeoutController.signal.addEventListener('abort', () => combinedController.abort());
    try {
      const response = await fetch("https://api.openai.com/v1/chat/completions", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${this.apiKey}`,
        },
        body: JSON.stringify({
          model,
          messages,
          stream: streamResponse,
          stream_options: streamResponse ? { include_usage: true } : undefined
        }),
        signal: combinedController.signal,
      });

      if (!response.ok) {
        throw new Error(`OpenAI API error: ${response.statusText} (${response.status})`);
      }

      if (!streamResponse) {
        const data = await response.json();
        const content = data.choices[0]?.message?.content || '';
        const usage = data.usage ? {
          prompt_tokens: data.usage.prompt_tokens,
          completion_tokens: data.usage.completion_tokens
        } : undefined;
        onStreamResponse(content);
        return { content, usage };
      }

      return this.handleStreamingResponse(response, onStreamResponse);
    } catch (error: any) {
      if (error?.name === 'AbortError') {
        throw new Error('Request timed out or was aborted');
      }
      throw error;
    } finally {
      clearTimeout(timeoutId);
    }
  }

  async completion(model: string, messages: {role: string, content: string, attachments?: Attachment[]}[], signal: AbortSignal): Promise<ChatCompletionResponse> {
     let openai = new ChatOpenAI({
      model,
      apiKey: this.apiKey,
    });
    let formattedMessages = messages.map((message) => ({role: message.role, content: message.content}));
    console.log('Formatted messages:', formattedMessages);
    return openai.invoke(formattedMessages, {signal: signal}).then((response: any) => {
        return {
            message: {message: response.content?.toString(), role: 'assistant'},
            usage: {
                totalTokens: response.usage_metadata?.total_tokens,
                promptTokens: response.usage_metadata?.input_tokens,
                completionTokens: response.usage_metadata?.output_tokens
            }
        };
    });
  }

  private async handleStreamingResponse(
    response: Response,
    onStreamResponse: (chunk: string) => void
  ): Promise<{ content: string; usage?: { prompt_tokens: number; completion_tokens: number } }> {
    const reader = response.body?.getReader();
    if (!reader) throw new Error('No response body reader available');

    let fullResponse = '';
    let usage: { prompt_tokens: number; completion_tokens: number } | undefined;
    const decoder = new TextDecoder();

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const lines = decoder.decode(value).split('\n');
        const parsedLines = lines
          .map(line => line.replace(/^data: /, '').trim())
          .filter(line => line && line !== '[DONE]')
          .map(line => {
            try {
              return JSON.parse(line);
            } catch {
              return null;
            }
          })
          .filter(data => data);

        for (const data of parsedLines) {
          // Extract text chunks
          if (data?.choices?.[0]?.delta?.content) {
            const chunk = data.choices[0].delta.content;
            fullResponse += chunk;
            onStreamResponse(chunk);
          }

          // Extract usage information (sent at the end of stream)
          if (data?.usage) {
            usage = {
              prompt_tokens: data.usage.prompt_tokens,
              completion_tokens: data.usage.completion_tokens
            };
          }
        }
      }
      return { content: fullResponse, usage };
    } finally {
      reader.releaseLock();
    }
  }

  async transcribeAudio(base64Audio: string, context: string): Promise<string> {
    // Convert base64 to blob
    const base64Data = base64Audio.split(',')[1];
    const binaryData = atob(base64Data);
    const bytes = new Uint8Array(binaryData.length);
    for (let i = 0; i < binaryData.length; i++) {
      bytes[i] = binaryData.charCodeAt(i);
    }
    const blob = new Blob([bytes], { type: 'audio/mp3' });

    // Create form data
    const formData = new FormData();
    formData.append('file', blob, 'audio.mp3');
    formData.append('model', 'whisper-1');
    formData.append('response_format', 'text');
    formData.append('prompt', context);
    const response = await fetch('https://api.openai.com/v1/audio/transcriptions', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
      },
      body: formData,
    });

    if (!response.ok) {
      console.error('Failed to transcribe audio:', response.statusText);
      throw new Error(`OpenAI API error: ${response.statusText}`);
    }

    return response.text();
  }
}

export type ChatCompletionResponse = {
  message: {
      message: string;
      role: string;
  };
  usage: {
      totalTokens?: number;
      promptTokens?: number;
      completionTokens?: number;
  };
};