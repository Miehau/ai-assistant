import type { ChatCompletionMessageParam } from "openai/resources/chat/completions";
import Anthropic from '@anthropic-ai/sdk';
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
  ClaudeStructuredOutput
} from '$lib/types/llm';

interface AnthropicMessage {
  role: "user" | "assistant";
  content:
    | string
    | Array<{
        type: "text" | "image";
        text?: string;
        source?: {
          type: "base64";
          media_type: string;
          data: string;
        };
      }>;
}

export class AnthropicService extends LLMService {
  private client: Anthropic;

  constructor(apiKey: string) {
    super(apiKey);
    this.client = new Anthropic({
      apiKey,
      dangerouslyAllowBrowser: true
    });
  }

  get providerName(): string {
    return 'anthropic';
  }

  get supportsStructuredOutputs(): boolean {
    return true;
  }

  /**
   * Standard completion using Claude
   */
  async completion(
    model: string,
    messages: LLMMessage[],
    options?: LLMCompletionOptions
  ): Promise<LLMResponse> {
    try {
      // Extract system message
      const systemMessage = messages.find((m) => m.role === "system");
      const systemContent = typeof systemMessage?.content === 'string'
        ? systemMessage.content
        : undefined;

      // Convert messages to Claude format
      const claudeMessages = messages
        .filter((m) => m.role !== "system")
        .map((msg) => this.convertToClaudeMessage(msg))
        .filter(Boolean) as Anthropic.MessageParam[];

      const response = await this.client.messages.create({
        model,
        messages: claudeMessages,
        system: systemContent,
        max_tokens: options?.max_tokens ?? 4096,
        temperature: options?.temperature,
        top_p: options?.top_p,
      });

      // Extract text content
      const textContent = response.content
        .filter((block): block is Anthropic.TextBlock => block.type === 'text')
        .map(block => block.text)
        .join('\n');

      return {
        message: textContent,
        role: 'assistant',
        usage: {
          promptTokens: response.usage.input_tokens,
          completionTokens: response.usage.output_tokens,
          totalTokens: response.usage.input_tokens + response.usage.output_tokens
        },
        finishReason: response.stop_reason === 'end_turn' ? 'stop' : response.stop_reason as any
      };
    } catch (error) {
      throw new LLMServiceError(
        'Claude completion failed',
        this.providerName,
        error
      );
    }
  }

  /**
   * Structured completion using Claude's output_format parameter
   * Requires anthropic-beta: structured-outputs-2025-11-13 header
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
      // Extract system message
      const systemMessage = messages.find((m) => m.role === "system");
      const systemContent = typeof systemMessage?.content === 'string'
        ? systemMessage.content
        : undefined;

      // Convert messages to Claude format
      const claudeMessages = messages
        .filter((m) => m.role !== "system")
        .map((msg) => this.convertToClaudeMessage(msg))
        .filter(Boolean) as Anthropic.MessageParam[];

      console.log('Claude structured completion with schema:', schema.name);

      // Use the native SDK with structured outputs
      // Note: As of the SDK version, we need to use the raw API for output_format
      const response = await fetch('https://api.anthropic.com/v1/messages', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': this.apiKey,
          'anthropic-version': '2023-06-01',
          'anthropic-beta': 'structured-outputs-2025-11-13',
          'anthropic-dangerous-direct-browser-access': 'true'
        },
        body: JSON.stringify({
          model,
          messages: claudeMessages,
          system: systemContent,
          max_tokens: options?.max_tokens ?? 4096,
          temperature: options?.temperature,
          top_p: options?.top_p,
          output_format: {
            type: 'json_schema',
            schema: schema.schema
          }
        }),
        signal: options?.signal
      });

      if (!response.ok) {
        const errorData = await response.json().catch(() => ({}));
        throw new Error(`Claude API error: ${response.statusText} - ${JSON.stringify(errorData)}`);
      }

      const data = await response.json();

      // Check for refusal
      if (data.stop_reason === 'refusal') {
        throw new RefusalError(
          'Claude refused to generate response',
          this.providerName,
          data.content?.[0]?.text || 'Unknown refusal reason'
        );
      }

      // Extract text content
      const rawResponse = data.content
        ?.filter((block: any) => block.type === 'text')
        .map((block: any) => block.text)
        .join('\n') || '';

      // Parse and validate JSON
      let parsedData: T;
      try {
        parsedData = JSON.parse(rawResponse);
      } catch (error) {
        throw new SchemaValidationError(
          'Failed to parse JSON response from Claude',
          this.providerName,
          schema,
          error
        );
      }

      return {
        data: parsedData,
        rawResponse,
        usage: data.usage ? {
          promptTokens: data.usage.input_tokens,
          completionTokens: data.usage.output_tokens,
          totalTokens: data.usage.input_tokens + data.usage.output_tokens
        } : undefined
      };
    } catch (error) {
      if (error instanceof LLMServiceError) {
        throw error;
      }
      throw new LLMServiceError(
        'Claude structured completion failed',
        this.providerName,
        error
      );
    }
  }

  /**
   * Convert LLMMessage to Claude message format
   */
  private convertToClaudeMessage(msg: LLMMessage): Anthropic.MessageParam | null {
    // Skip system messages
    if (msg.role === 'system' || msg.role === 'tool') {
      return null;
    }

    // Handle string content
    if (typeof msg.content === 'string') {
      return {
        role: msg.role as 'user' | 'assistant',
        content: msg.content
      };
    }

    // Handle array content (with images)
    const claudeContent = msg.content
      .map((part) => {
        if (part.type === 'text') {
          return {
            type: 'text' as const,
            text: part.text || ''
          };
        }

        if (part.type === 'image_url') {
          // Extract base64 data and media type from data URL
          const dataUrl = part.image_url?.url || '';
          const match = dataUrl.match(
            /^data:(image\/(jpeg|png|gif|webp));base64,(.+)$/
          );

          if (match) {
            return {
              type: 'image' as const,
              source: {
                type: 'base64' as const,
                media_type: match[1] as 'image/jpeg' | 'image/png' | 'image/gif' | 'image/webp',
                data: match[3]
              }
            };
          }
        }

        return null;
      })
      .filter(Boolean) as Anthropic.MessageParam['content'];

    return {
      role: msg.role as 'user' | 'assistant',
      content: claudeContent
    };
  }

  /**
   * Legacy method - kept for backward compatibility
   * @deprecated Use completion() instead
   */
  private convertOpenAIMessageToAnthropic(
    msg: ChatCompletionMessageParam
  ): AnthropicMessage | null {
    // Skip system messages - they're handled separately
    if (msg.role === "system") {
      return null;
    }

    // Handle string content
    if (typeof msg.content === "string") {
      return {
        role: msg.role as "user" | "assistant",
        content: msg.content,
      };
    }

    // Handle array content (with images)
    if (Array.isArray(msg.content)) {
      const anthropicContent = msg.content
        .map((part) => {
          if (part.type === "text") {
            return {
              type: "text" as const,
              text: part.text || "",
            };
          }

          if (part.type === "image_url") {
            // Extract base64 data and media type from data URL
            const dataUrl = part.image_url?.url || "";
            const match = dataUrl.match(
              /^data:(image\/(jpeg|png|gif|webp));base64,(.+)$/,
            );

            if (match) {
              return {
                type: "image" as const,
                source: {
                  type: "base64" as const,
                  media_type: match[1], // e.g., 'image/jpeg'
                  data: match[3], // base64 data without prefix
                },
              };
            }
          }

          return null;
        })
        .filter(Boolean) as Array<{
        type: "text" | "image";
        text?: string;
        source?: {
          type: "base64";
          media_type: string;
          data: string;
        };
      }>;

      return {
        role: msg.role as "user" | "assistant",
        content: anthropicContent,
      };
    }

    return null;
  }

  /**
   * Legacy method - kept for backward compatibility
   * @deprecated Use completion() instead
   */
  async createChatCompletion(
    model: string,
    messages: ChatCompletionMessageParam[],
    stream: boolean,
    onStreamResponse: (chunk: string) => void,
    signal: AbortSignal,
  ): Promise<{ content: string; usage?: { prompt_tokens: number; completion_tokens: number } }> {
    // Extract system message
    const systemMessage = messages.find((m) => m.role === "system");
    const systemContent = systemMessage?.content;

    // Convert non-system messages
    const userMessages = messages
      .filter((m) => m.role !== "system")
      .map((msg) => this.convertOpenAIMessageToAnthropic(msg))
      .filter(Boolean) as AnthropicMessage[];

    // Ensure messages alternate between user and assistant, starting with user
    const formattedMessages: AnthropicMessage[] = [];
    let lastRole: "user" | "assistant" | null = null;

    for (const msg of userMessages) {
      if (msg.role === lastRole) {
        // Merge consecutive messages from the same role
        const lastMsg = formattedMessages[formattedMessages.length - 1];
        if (
          typeof lastMsg.content === "string" &&
          typeof msg.content === "string"
        ) {
          lastMsg.content = lastMsg.content + "\n\n" + msg.content;
        } else if (
          Array.isArray(lastMsg.content) &&
          Array.isArray(msg.content)
        ) {
          lastMsg.content = [...lastMsg.content, ...msg.content];
        }
      } else {
        formattedMessages.push(msg);
        lastRole = msg.role;
      }
    }

    const body = {
      model,
      messages: formattedMessages,
      system: typeof systemContent === "string" ? systemContent : undefined,
      stream,
      max_tokens: 4096,
    };

    const response = await fetch("https://api.anthropic.com/v1/messages", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-api-key": this.apiKey,
        "anthropic-version": "2023-06-01",
        "anthropic-dangerous-direct-browser-access": "true",
      },
      body: JSON.stringify(body),
      signal,
    });

    if (!response.ok) {
      const data = await response.json();
      throw new Error(`Anthropic API error: ${JSON.stringify(data)}`);
    }

    if (stream) {
      const reader = response.body?.getReader();
      const decoder = new TextDecoder();
      let fullText = "";
      let usage: { prompt_tokens: number; completion_tokens: number } | undefined;

      if (!reader) throw new Error("No reader available");

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value);
        const lines = chunk.split("\n").filter((line) => line.trim());

        for (const line of lines) {
          if (line.startsWith("data: ")) {
            const data = line.slice(6);
            if (data === "[DONE]") continue;

            try {
              const parsed = JSON.parse(data);

              // Handle content_block_delta events with text
              if (
                parsed.type === "content_block_delta" &&
                parsed.delta?.type === "text_delta"
              ) {
                const text = parsed.delta.text || "";
                fullText += text;
                onStreamResponse(text);
              }

              // Handle message_delta events with usage
              if (parsed.type === "message_delta" && parsed.usage) {
                usage = {
                  prompt_tokens: parsed.usage.input_tokens || 0,
                  completion_tokens: parsed.usage.output_tokens || 0
                };
              }
            } catch (e) {
              console.error("Error parsing stream:", e);
            }
          }
        }
      }

      return { content: fullText, usage };
    } else {
      const data = await response.json();
      const text = data.content[0]?.text || "";
      const usage = data.usage ? {
        prompt_tokens: data.usage.input_tokens,
        completion_tokens: data.usage.output_tokens
      } : undefined;
      onStreamResponse(text);
      return { content: text, usage };
    }
  }

  /**
   * Audio transcription - not supported by Claude
   */
  async transcribeAudio(base64Audio: string, context: string): Promise<string> {
    throw new LLMServiceError(
      'Audio transcription not supported by Claude',
      this.providerName
    );
  }
}
