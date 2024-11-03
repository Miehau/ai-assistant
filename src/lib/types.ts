
export interface SystemPrompt {
    id: string;
    name: string;
    content: string;
    created_at: string;  // ISO string from backend
    updated_at: string;  // ISO string from backend
}


export type Attachment = {
    attachment_type: 'image';
    name: string;
    data: string;
    description?: string;
};

export type Message = {
    type: "sent" | "received";
    content: string;
    attachments?: Attachment[];
};