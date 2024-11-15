import type { Message } from '$lib/types';
import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions';

export interface FormattedMessage {
  role: 'system' | 'user' | 'assistant';
  content: string | Array<{ type: string; text?: string; image_url?: { url: string; detail: string } }>;
}

export async function formatMessages(history: any[], currentMessage: Message, systemPrompt?: string): Promise<ChatCompletionMessageParam[]> {
  return [
    {
      role: 'system',
      content: systemPrompt || "You are a helpful AI assistant."
    },
    ...history.map(formatHistoryMessage),
    await formatUserMessage(currentMessage)
  ];
}

export function formatHistoryMessage(msg: any): ChatCompletionMessageParam {
  return {
    role: msg.role,
    content: msg.attachments ? [
      { type: "text", text: msg.content },
      ...msg.attachments.map((att: any) => ({
        type: "image_url",
        image_url: {
          url: `${att.data}`,
          detail: "auto"
        }
      }))
    ] : msg.content
  };
}

export async function formatUserMessage(message: Message): Promise<ChatCompletionMessageParam> {
  if (!message.attachments) {
    return {
      role: 'user',
      content: message.content
    };
  }

  const audioMessages = message.attachments
    .filter(att => att.attachment_type.startsWith("audio"))
    .map(att => `[Attached audio message transcript: ${att.transcript}]`)
    .join("\n");

  const content = [
    { type: "text" as const, text: message.content + (audioMessages ? "\n" + audioMessages : "") },
    ...message.attachments
      .filter(att => !att.attachment_type.startsWith("audio"))
      .map(att => ({
        type: "image_url" as const,
        image_url: {
          url: att.data,
          detail: "auto"
        }
      }))
  ];

  return {
    role: 'user',
    content
  };
} 