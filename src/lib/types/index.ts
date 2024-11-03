export interface Message {
  content: string;
  attachments?: {
    type: string;
    name: string;
    data: string;
    description?: string;
  }[];
} 