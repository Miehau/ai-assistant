/**
 * Type converters for message transformations
 *
 * These functions provide safe conversions between different message representations:
 * - Display ↔ DB: UI components ↔ Database
 * - Display → API: UI → Network requests
 * - API → Display: Network responses → UI
 */

import type { DisplayMessage, APIMessage, DBMessage } from './message';

/**
 * Convert database message to display message
 * Used when loading messages from the database for UI rendering
 *
 * @param db - Database message
 * @returns Display message for UI
 */
export function toDisplayMessage(db: DBMessage): DisplayMessage {
  return {
    id: db.id!,  // Assert non-null since DB messages should have IDs
    type: db.role === 'user' ? 'sent' : 'received',
    content: db.content,
    attachments: db.attachments,
    timestamp: db.timestamp
  };
}

/**
 * Convert display message to API message
 * Used when preparing messages for AI provider requests
 *
 * @param display - Display message from UI
 * @returns API message for network request
 */
export function toAPIMessage(display: DisplayMessage): APIMessage {
  return {
    role: display.type === 'sent' ? 'user' : 'assistant',
    content: display.content,
    attachments: display.attachments
  };
}

/**
 * Convert database message to API message
 * Used when loading conversation history for AI context
 *
 * @param db - Database message
 * @returns API message for network request
 */
export function dbToAPIMessage(db: DBMessage): APIMessage {
  return {
    role: db.role,
    content: db.content,
    attachments: db.attachments
  };
}

/**
 * Convert API message response to display message
 * Used when receiving AI responses to show in UI
 *
 * @param api - API message from provider response
 * @param id - Message ID (generated externally)
 * @returns Display message for UI
 */
export function apiToDisplayMessage(
  api: APIMessage,
  id: string
): DisplayMessage {
  return {
    id,
    type: api.role === 'user' ? 'sent' : 'received',
    content: api.content,
    attachments: api.attachments,
    timestamp: Date.now()
  };
}

/**
 * Convert display message to database message
 * Used when persisting messages to the database
 *
 * @param display - Display message from UI
 * @returns Database message for persistence
 */
export function toDBMessage(display: DisplayMessage): DBMessage {
  return {
    id: display.id,
    role: display.type === 'sent' ? 'user' : 'assistant',
    content: display.content,
    attachments: display.attachments,
    timestamp: display.timestamp
  };
}

/**
 * Batch convert database messages to display messages
 * Convenience function for loading conversation history
 *
 * @param dbMessages - Array of database messages
 * @returns Array of display messages
 */
export function toDisplayMessages(dbMessages: DBMessage[]): DisplayMessage[] {
  return dbMessages.map(toDisplayMessage);
}

/**
 * Batch convert display messages to API messages
 * Convenience function for preparing conversation context
 *
 * @param displayMessages - Array of display messages
 * @returns Array of API messages
 */
export function toAPIMessages(displayMessages: DisplayMessage[]): APIMessage[] {
  return displayMessages.map(toAPIMessage);
}

/**
 * Batch convert database messages to API messages
 * Convenience function for loading history for AI context
 *
 * @param dbMessages - Array of database messages
 * @returns Array of API messages
 */
export function dbToAPIMessages(dbMessages: DBMessage[]): APIMessage[] {
  return dbMessages.map(dbToAPIMessage);
}
