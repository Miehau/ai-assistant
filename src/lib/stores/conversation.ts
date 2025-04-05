// src/lib/stores/conversation.ts
import { writable } from 'svelte/store';
import type { Conversation } from '$lib/types';

// Store for the current conversation
export const currentConversation = writable<Conversation | null>(null);
