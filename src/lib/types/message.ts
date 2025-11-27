/**
 * Unified Message Type System
 *
 * This file defines a clear hierarchy of message types for different layers:
 * - BaseMessage: Core message properties (always has ID)
 * - DisplayMessage: UI layer representation
 * - APIMessage: Network layer representation
 * - DBMessage: Persistence layer representation
 *
 * Key principles:
 * - IDs are REQUIRED (not optional) - prevents branching bugs
 * - Clear separation between layers
 * - Type-safe conversions between layers
 */

import type { Attachment } from './attachments';

/**
 * Base message interface with core properties
 * All messages should have an ID and timestamp for proper tracking
 */
export interface BaseMessage {
  /** Unique message identifier (required) */
  id: string;
  /** Message content */
  content: string;
  /** Unix timestamp in milliseconds (optional for backward compatibility) */
  timestamp?: number;
}

/**
 * Display message for UI rendering
 * Used in components and stores
 *
 * @note timestamp is optional for backward compatibility but should be provided
 */
export interface DisplayMessage extends BaseMessage {
  /** Message direction for UI display */
  type: 'sent' | 'received';
  /** Optional file/image attachments */
  attachments?: Attachment[];
  /** Optional model name for display (e.g., "gpt-4 • openai") */
  model?: string;
}

/**
 * API message for network communication
 * Used when sending/receiving from AI providers
 */
export interface APIMessage {
  /** Role in the conversation */
  role: 'user' | 'assistant' | 'system';
  /** Message content */
  content: string;
  /** Optional attachments */
  attachments?: Attachment[];
}

/**
 * Database message from persistence layer
 * Used when loading/saving to database
 */
export interface DBMessage {
  /** Unique message identifier */
  id?: string;
  /** Role in the conversation */
  role: 'user' | 'assistant';
  /** Message content */
  content: string;
  /** Optional attachments */
  attachments?: Attachment[];
  /** Unix timestamp in milliseconds (optional for backward compatibility) */
  timestamp?: number;
}

/**
 * Extended message with branching information
 * Used in branch visualization and management
 */
export interface MessageWithBranch extends DisplayMessage {
  /** Whether this message is a branch point */
  is_branch_point?: boolean;
  /** Number of branches from this message */
  branch_count?: number;
  /** Whether this message has child branches */
  has_branches?: boolean;
}

/**
 * Message usage tracking information
 */
export interface MessageUsage {
  id: string;
  message_id: string;
  model_name: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  estimated_cost: number;
  created_at: string;
}

/**
 * Message tree node for branch management
 */
export interface MessageTreeNode {
  message_id: string;
  parent_message_id: string | null;
  branch_id: string;
  branch_point: boolean;
  created_at: string;
}

/**
 * Type guards for runtime type checking
 */

export function isDisplayMessage(msg: unknown): msg is DisplayMessage {
  return (
    typeof msg === 'object' &&
    msg !== null &&
    'id' in msg &&
    'type' in msg &&
    'content' in msg &&
    'timestamp' in msg &&
    (msg as DisplayMessage).type in ['sent', 'received']
  );
}

export function isAPIMessage(msg: unknown): msg is APIMessage {
  return (
    typeof msg === 'object' &&
    msg !== null &&
    'role' in msg &&
    'content' in msg &&
    (msg as APIMessage).role in ['user', 'assistant', 'system']
  );
}

export function isDBMessage(msg: unknown): msg is DBMessage {
  return (
    typeof msg === 'object' &&
    msg !== null &&
    'id' in msg &&
    'role' in msg &&
    'content' in msg &&
    'timestamp' in msg &&
    (msg as DBMessage).role in ['user', 'assistant']
  );
}
