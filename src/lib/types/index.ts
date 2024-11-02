export type Attachment = {
  type: 'image';
  name: string;
  data: string;
};

export type Message = {
  type: "sent" | "received";
  content: string;
  attachments?: Attachment[];
}; 