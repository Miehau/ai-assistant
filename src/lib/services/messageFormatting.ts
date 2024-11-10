import type { Message } from '$lib/types';

export interface FormattedMessage {
  role: 'system' | 'user' | 'assistant';
  content: string | Array<{ type: string; text?: string; image_url?: { url: string; detail: string } }>;
}

export function formatMessages(history: any[], currentMessage: Message, systemPrompt?: string): FormattedMessage[] {
  return [
    { 
      role: 'system', 
      content: systemPrompt || "You are a helpful AI assistant."
    },
    ...history.map(formatHistoryMessage),
    formatUserMessage(currentMessage)
  ];
}

export function formatHistoryMessage(msg: any): FormattedMessage {
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

export function formatUserMessage(message: Message): FormattedMessage {
  return {
    role: 'user',
    content: message.attachments ? [
      { type: "text", text: message.content },
      ...message.attachments.map(att => ({
        type: "image_url",
        image_url: {
          url: `${att.data}`,
          detail: "auto"
        }
      }))
    ] : message.content
  };
} 