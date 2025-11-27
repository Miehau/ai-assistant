import type { Message } from '$lib/types';
import { formatMessages } from './messageFormatting';
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
import type { CustomBackend } from '$lib/types/customBackend';

export interface CustomProviderConfig {
  url: string;
  apiKey?: string;
  name?: string;
}

export class CustomProviderService extends LLMService {
  private baseUrl: string;
  private backendName: string;

  constructor(config: CustomProviderConfig) {
    super(config.apiKey || '');
    this.baseUrl = config.url;
    this.backendName = config.name || 'custom';
  }

  /**
   * Create a CustomProviderService from a CustomBackend
   */
  static fromBackend(backend: CustomBackend): CustomProviderService {
    return new CustomProviderService({
      url: backend.url,
      apiKey: backend.api_key,
      name: backend.name
    });
  }

  get providerName(): string {
    return this.backendName;
  }

  get supportsStructuredOutputs(): boolean {
    // Custom providers may or may not support structured outputs
    // Default to false, can be enabled if endpoint supports it
    return false;
  }
  async createChatCompletion(
    modelName: string,
    url: string,
    messages: any[],
    streamResponse: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal
  ): Promise<string> {
    const body = JSON.stringify({
      messages,
      model: modelName,
      stream: streamResponse,
    });

    // Create a timeout controller if one wasn't provided
    const timeoutController = new AbortController();
    const timeoutId = setTimeout(() => timeoutController.abort(), 180000); // 3 minutes

    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body,
        // Combine both signals if an external one was provided
        signal: signal ? AbortSignal.any([signal, timeoutController.signal]) : timeoutController.signal,
      });

      if (!response.ok) {
        throw new Error(`Failed to send chat message to custom provider: ${response.statusText}`);
      }

      const isStreaming = response.headers.get('content-type')?.includes('text/event-stream') 
        || response.headers.get('content-type')?.includes('application/x-ndjson')
        || response.headers.get('transfer-encoding')?.includes('chunked');

      if (streamResponse && isStreaming && response.body) {
        return this.handleStreamingResponse(response, onStreamResponse);
      }
      const data = await response.json();
      const content = data.message?.content || data.choices?.[0]?.message?.content || '';
      if (onStreamResponse) {
        onStreamResponse(content);
      }
      return content;
    } finally {
      clearTimeout(timeoutId);
    }
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

  /**
   * Standard completion method using new unified interface
   */
  async completion(
    model: string,
    messages: LLMMessage[],
    options?: LLMCompletionOptions
  ): Promise<LLMResponse> {
    try {
      const formattedMessages = messages.map((message) => ({
        role: message.role,
        content: typeof message.content === 'string' ? message.content : JSON.stringify(message.content)
      }));

      console.log('Custom provider completion with messages:', formattedMessages);

      const timeoutController = new AbortController();
      const timeoutId = setTimeout(() => timeoutController.abort(), 180000); // 3 minutes

      try {
        const apiResponse = await fetch(this.baseUrl, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            ...(this.apiKey && { Authorization: `Bearer ${this.apiKey}` }),
          },
          body: JSON.stringify({
            model,
            messages: formattedMessages,
            temperature: options?.temperature,
            max_tokens: options?.max_tokens,
            top_p: options?.top_p,
            stream: false,
          }),
          signal: options?.signal ? AbortSignal.any([options.signal, timeoutController.signal]) : timeoutController.signal,
        });

        if (!apiResponse.ok) {
          const errorData = await apiResponse.json().catch(() => ({}));
          throw new Error(`Custom Provider API error: ${apiResponse.statusText} - ${JSON.stringify(errorData)}`);
        }

        const data = await apiResponse.json();

        // Try different response formats (OpenAI-compatible or custom)
        const content = data.message?.content || data.choices?.[0]?.message?.content || '';
        const usage = data.usage ? {
          totalTokens: data.usage.total_tokens || 0,
          promptTokens: data.usage.prompt_tokens || 0,
          completionTokens: data.usage.completion_tokens || 0
        } : undefined;

        return {
          message: content,
          role: 'assistant',
          usage,
          finishReason: data.choices?.[0]?.finish_reason || 'stop'
        };
      } finally {
        clearTimeout(timeoutId);
      }
    } catch (error) {
      throw new LLMServiceError(
        'Custom provider completion failed',
        this.providerName,
        error
      );
    }
  }

  /**
   * Structured completion - not supported by default for custom providers
   */
  async structuredCompletion<T = any>(
    model: string,
    messages: LLMMessage[],
    schema: StructuredOutputSchema,
    options?: Omit<LLMCompletionOptions, 'structuredOutput'>
  ): Promise<LLMStructuredResponse<T>> {
    // If endpoint supports OpenAI-compatible structured outputs, try to use them
    // Otherwise, throw an error
    throw new LLMServiceError(
      'Structured outputs not supported for custom provider. Configure your endpoint to support OpenAI-compatible structured outputs.',
      this.providerName
    );
  }

  async transcribeAudio(base64Audio: string, context: string): Promise<string> {
    throw new LLMServiceError(
      'Audio transcription not supported for custom provider',
      this.providerName
    );
  }
}

/**
 * @deprecated Use CustomProviderService.fromBackend() instead
 * This singleton is kept for backwards compatibility
 */
export const customProviderService = new CustomProviderService({ url: '', apiKey: '' }); 