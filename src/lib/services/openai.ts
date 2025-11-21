import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';
import { ChatOpenAI } from '@langchain/openai';
import type { Attachment } from '$lib/types';
import {
  LLMService,
  type LLMStructuredResponse,
  LLMServiceError,
  SchemaValidationError,
  RefusalError
} from './base/LLMService';
import type {
  LLMMessage,
  LLMResponse,
  LLMCompletionOptions,
  StructuredOutputSchema,
  OpenAIStructuredOutput
} from '$lib/types/llm';

export class OpenAIService extends LLMService {
  get providerName(): string {
    return 'openai';
  }

  get supportsStructuredOutputs(): boolean {
    return true;
  }

  /**
   * Legacy method - kept for backward compatibility
   * @deprecated Use completion() instead
   */
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

  /**
   * Standard completion with LangChain
   */
  async completion(
    model: string,
    messages: LLMMessage[],
    options?: LLMCompletionOptions
  ): Promise<LLMResponse> {
    try {
      const openai = new ChatOpenAI({
        model,
        apiKey: this.apiKey,
        temperature: options?.temperature,
        maxTokens: options?.max_tokens,
        topP: options?.top_p,
      });

      const formattedMessages = messages.map((message) => ({
        role: message.role,
        content: typeof message.content === 'string' ? message.content : JSON.stringify(message.content)
      }));

      console.log('OpenAI completion with messages:', formattedMessages);

      const response = await openai.invoke(formattedMessages, {
        signal: options?.signal
      });

      return {
        message: response.content?.toString() || '',
        role: 'assistant',
        usage: {
          totalTokens: response.usage_metadata?.total_tokens || 0,
          promptTokens: response.usage_metadata?.input_tokens || 0,
          completionTokens: response.usage_metadata?.output_tokens || 0
        },
        finishReason: 'stop'
      };
    } catch (error) {
      throw new LLMServiceError(
        'OpenAI completion failed',
        this.providerName,
        error
      );
    }
  }

  /**
   * Structured completion using OpenAI's JSON Schema mode
   */
  async structuredCompletion<T = any>(
    model: string,
    messages: LLMMessage[],
    schema: StructuredOutputSchema,
    options?: Omit<LLMCompletionOptions, 'structuredOutput'>
  ): Promise<LLMStructuredResponse<T>> {
    // Validate schema
    this.validateSchema(schema);

    try {
      // Convert to OpenAI format
      const response_format: OpenAIStructuredOutput = {
        type: 'json_schema',
        json_schema: {
          name: schema.name || 'response',
          schema: schema.schema,
          strict: schema.strict ?? true
        }
      };

      const formattedMessages = messages.map((message) => ({
        role: message.role,
        content: typeof message.content === 'string' ? message.content : JSON.stringify(message.content)
      }));

      console.log('OpenAI structured completion with schema:', schema.name);

      const apiResponse = await fetch("https://api.openai.com/v1/chat/completions", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${this.apiKey}`,
        },
        body: JSON.stringify({
          model,
          messages: formattedMessages,
          response_format,
          temperature: options?.temperature,
          max_tokens: options?.max_tokens,
          top_p: options?.top_p,
        }),
        signal: options?.signal,
      });

      if (!apiResponse.ok) {
        const errorData = await apiResponse.json().catch(() => ({}));
        throw new Error(`OpenAI API error: ${apiResponse.statusText} - ${JSON.stringify(errorData)}`);
      }

      const data = await apiResponse.json();
      const choice = data.choices[0];

      // Check for refusal
      if (choice?.finish_reason === 'refusal' || choice?.message?.refusal) {
        throw new RefusalError(
          'OpenAI refused to generate response',
          this.providerName,
          choice.message?.refusal || 'Unknown refusal reason'
        );
      }

      const rawResponse = choice?.message?.content || '';

      // Parse and validate JSON
      let parsedData: T;
      try {
        parsedData = JSON.parse(rawResponse);
      } catch (error) {
        throw new SchemaValidationError(
          'Failed to parse JSON response from OpenAI',
          this.providerName,
          schema,
          error
        );
      }

      return {
        data: parsedData,
        rawResponse,
        usage: data.usage ? {
          promptTokens: data.usage.prompt_tokens,
          completionTokens: data.usage.completion_tokens,
          totalTokens: data.usage.total_tokens
        } : undefined
      };
    } catch (error) {
      if (error instanceof LLMServiceError) {
        throw error;
      }
      throw new LLMServiceError(
        'OpenAI structured completion failed',
        this.providerName,
        error
      );
    }
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
