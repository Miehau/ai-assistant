/**
 * @deprecated This file is deprecated. Import from types/message.ts instead.
 * Kept for backward compatibility during migration.
 */

// Re-export from the new unified type system
export type { DisplayMessage, APIMessage, DBMessage } from './message';
export type { Attachment } from './attachments';
export type { CustomBackend, CreateCustomBackendInput, UpdateCustomBackendInput } from './customBackend';

// Temporary alias for backward compatibility
export type { DisplayMessage as Message } from './message'; 