import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';
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

export class DeepSeekService extends LLMService {
  get providerName(): string {
    return 'deepseek';
  }

  get supportsStructuredOutputs(): boolean {
    return true; // DeepSeek supports OpenAI-compatible structured outputs
  }

  async createChatCompletion(
    model: string,
    messages: ChatCompletionMessageParam[],
    streamResponse: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal
  ): Promise<{ content: string; usage?: { prompt_tokens: number; completion_tokens: number } }> {
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
      const usage = data.usage ? {
        prompt_tokens: data.usage.prompt_tokens ?? 0,
        completion_tokens: data.usage.completion_tokens ?? 0
      } : undefined;
      onStreamResponse(content);
      return { content, usage };
    }

    return this.handleStreamingResponse(response, onStreamResponse);
  }

  private async handleStreamingResponse(
    response: Response, 
    onStreamResponse: (chunk: string) => void
  ): Promise<{ content: string; usage?: { prompt_tokens: number; completion_tokens: number } }> {
    const reader = response.body?.getReader();
    if (!reader) throw new Error('No response body reader available');
    
    let fullResponse = '';
    const decoder = new TextDecoder();
    let usage: { prompt_tokens: number; completion_tokens: number } | undefined;

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
          if (data?.choices?.[0]?.delta?.content) {
            const chunk = data.choices[0].delta.content;
            fullResponse += chunk;
            onStreamResponse(chunk);
          }

          if (data?.usage) {
            usage = {
              prompt_tokens: data.usage.prompt_tokens ?? usage?.prompt_tokens ?? 0,
              completion_tokens: data.usage.completion_tokens ?? usage?.completion_tokens ?? 0
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
    throw new Error(`DeepSeek API error: Not implemented`);
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

      console.log('DeepSeek completion with messages:', formattedMessages);

      const apiResponse = await fetch("https://api.deepseek.com/v1/chat/completions", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${this.apiKey}`,
        },
        body: JSON.stringify({
          model,
          messages: formattedMessages,
          temperature: options?.temperature,
          max_tokens: options?.max_tokens,
          top_p: options?.top_p,
          stream: false,
        }),
        signal: options?.signal,
      });

      if (!apiResponse.ok) {
        const errorData = await apiResponse.json().catch(() => ({}));
        throw new Error(`DeepSeek API error: ${apiResponse.statusText} - ${JSON.stringify(errorData)}`);
      }

      const data = await apiResponse.json();
      const choice = data.choices[0];

      return {
        message: choice?.message?.content || '',
        role: 'assistant',
        usage: data.usage ? {
          totalTokens: data.usage.total_tokens,
          promptTokens: data.usage.prompt_tokens,
          completionTokens: data.usage.completion_tokens
        } : undefined,
        finishReason: choice?.finish_reason || 'stop'
      };
    } catch (error) {
      throw new LLMServiceError(
        'DeepSeek completion failed',
        this.providerName,
        error
      );
    }
  }

  /**
   * Structured completion using OpenAI-compatible JSON Schema mode
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
      // Use OpenAI-compatible format
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

      console.log('DeepSeek structured completion with schema:', schema.name);

      const apiResponse = await fetch("https://api.deepseek.com/v1/chat/completions", {
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
        throw new Error(`DeepSeek API error: ${apiResponse.statusText} - ${JSON.stringify(errorData)}`);
      }

      const data = await apiResponse.json();
      const choice = data.choices[0];

      // Check for refusal
      if (choice?.finish_reason === 'refusal' || choice?.message?.refusal) {
        throw new RefusalError(
          'DeepSeek refused to generate response',
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
          'Failed to parse JSON response from DeepSeek',
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
        'DeepSeek structured completion failed',
        this.providerName,
        error
      );
    }
  }
}
