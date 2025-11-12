import type { ChatCompletionMessageParam } from "openai/resources/chat/completions";

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

export class AnthropicService {
  constructor(private apiKey: string) {}

  private convertOpenAIMessageToAnthropic(
    msg: ChatCompletionMessageParam,
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
}
