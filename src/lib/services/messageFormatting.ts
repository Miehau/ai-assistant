import type { Message } from '$lib/types';

export interface FormattedMessage {
  role: 'system' | 'user' | 'assistant';
  content: string | Array<{ type: string; text?: string; image_url?: { url: string; detail: string } }>;
}

export async function formatMessages(history: any[], currentMessage: Message, systemPrompt?: string): FormattedMessage[] {
  return [
    { 
      role: 'system', 
      content: systemPrompt || "You are a helpful AI assistant."
    },
    ...history.map(formatHistoryMessage),
    await formatUserMessage(currentMessage)
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

export async function formatUserMessage(message: Message): Promise<FormattedMessage> {
  if (!message.attachments) {
    return {
      role: 'user',
      content: message.content
    };
  }

  const content = [
    { type: "text", text: message.content },
    ...await Promise.all(message.attachments.map(async att => ({
      type: "image_url",
      image_url: {
        url: await resizeImage(att.data),
        detail: "auto"
      }
    })))
  ];
  console.log(content);

  return {
    role: 'user',
    content
  };
}

async function resizeImage(dataUrl: string, maxWidth: number = 2048, maxHeight: number = 2048): Promise<string> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      // If image is smaller than max dimensions, return original
      if (img.width <= maxWidth && img.height <= maxHeight) {
        resolve(dataUrl);
        return;
      }

      const canvas = document.createElement('canvas');
      // Calculate scale based on both width and height constraints
      const scaleWidth = maxWidth / img.width;
      const scaleHeight = maxHeight / img.height;
      const scale = Math.min(scaleWidth, scaleHeight);
      console.log(scale);

      canvas.width = img.width * scale;
      canvas.height = img.height * scale;

      const ctx = canvas.getContext('2d');
      ctx?.drawImage(img, 0, 0, canvas.width, canvas.height);
      resolve(canvas.toDataURL('image/jpeg', 0.8));
    };
    img.src = dataUrl;
  });
} 