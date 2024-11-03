import { invoke } from '@tauri-apps/api/tauri';
import { writable, get } from 'svelte/store';
import type { Message } from '$lib/types';

interface Conversation {
  id: string;
  name: string;
}

interface ConversationState {
  currentConversationId: string | null;
  currentConversation: Conversation | null;
}

export class ConversationService {
  private state = writable<ConversationState>({
    currentConversationId: null,
    currentConversation: null
  });

  // Subscribe to state changes
  subscribe = this.state.subscribe;

  async setCurrentConversation(conversationId: string | null) {
    const conversation = await this.getOrCreate(conversationId);
    this.state.update(state => ({
      ...state,
      currentConversationId: conversation.id,
      currentConversation: conversation
    }));
    return conversation;
  }

  getCurrentConversation(): Conversation | null {
    return get(this.state).currentConversation;
  }

  async getOrCreate(conversationId: string | null): Promise<Conversation> {
    const conversation = await invoke('get_or_create_conversation', { conversationId });
    return conversation;
  }

  async getHistory(conversationId: string): Promise<Message[]> {
    return await invoke('get_conversation_history', { conversationId });
  }

  async saveMessage(
    role: 'user' | 'assistant', 
    content: string,
    attachments: any[] = [],
    conversationId?: string
  ): Promise<void> {
    const currentState = get(this.state);
    const targetConversationId = conversationId || currentState.currentConversationId;
    
    if (!targetConversationId) {
      throw new Error('No conversation selected');
    }

    await invoke('save_message', { 
      conversationId: targetConversationId, 
      role, 
      content,
      attachments 
    });
  }

  async getAllConversations(): Promise<Conversation[]> {
    return await invoke('get_conversations');
  }
}

export const conversationService = new ConversationService();