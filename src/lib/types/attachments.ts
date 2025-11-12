/**
 * Attachment types for messages
 */

export interface FileMetadata {
  id: string;
  name: string;
  path: string;
  mime_type: string;
  size_bytes: number;
  created_at: string;
  updated_at: string;
  thumbnail_path?: string;
  metadata?: Record<string, unknown>;
}

export interface Attachment {
  id?: string;
  message_id?: string;
  name: string;
  data: string;
  attachment_url?: string;
  attachment_type: "image" | "audio" | "text";
  description?: string;
  created_at?: Date;
  transcript?: string;
  // Fields for file-based attachments
  file_path?: string;
  file_metadata?: FileMetadata;
}
