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
      ...msg.attachments.map((att: any) => {
        if (att.attachment_type.startsWith('text/')) {
          return { 
            type: "text", 
            text: `\n\n[Attached file: ${att.name}]\n\`\`\`\n${att.data}\n\`\`\`\n` 
          };
        }
        return {
          type: "image_url",
          image_url: {
            url: `${att.data}`,
            detail: "auto"
          }
        };
      })
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
    .map(att => `[Audio Transcript: ${att.name}\n\`\`\`\n${att.transcript}\`\`\`\n]`)
    .join("\n");

  console.log(`message.attachments: ${JSON.stringify(message.attachments)}`);
  const textAttachments = message.attachments
    .filter(att => att.attachment_type.startsWith("text"))
    .map(att => `\n\n[Attached file: ${att.name}]\n\`\`\`\n${att.data}\n\`\`\`\n`)
    .join("");

  const jsonAttachments = message.attachments
    .filter(att => att.attachment_type.startsWith("application/json"))
    .map(att => `\n\n[Attached JSON: ${att.name}]\n\`\`\`\n${att.data}\n\`\`\`\n`)
    .join("");

  const content = [
    { 
      type: "text" as const,
      text: message.content 
      + (audioMessages ? "\n" + audioMessages : "")
      + textAttachments
      + jsonAttachments
      + (message.attachments.some(att => att.attachment_type.startsWith("image")) 
         ? "\n\n[Attached images: " + message.attachments
           .filter(att => att.attachment_type.startsWith("image"))
           .map(att => att.name)
           .join(", ") + "]"
         : "")
    },
    ...message.attachments
      .filter(att => att.attachment_type.startsWith("image"))
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