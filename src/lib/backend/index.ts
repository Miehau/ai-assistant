/**
 * Backend module - Centralized Tauri backend communication layer
 *
 * Usage:
 * ```typescript
 * import { backend } from '$lib/backend';
 *
 * // Get all models
 * const models = await backend.getModels();
 *
 * // Save a message
 * const messageId = await backend.saveMessage(conversationId, 'user', 'Hello!');
 * ```
 */

export { backend, BackendClient } from './client';
