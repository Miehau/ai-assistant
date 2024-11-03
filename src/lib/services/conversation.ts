import { invoke } from '@tauri-apps/api/tauri';
import { writable, get } from 'svelte/store';
import type { Message, APIMessage, Attachment, Conversation, ConversationState, DBMessage } from '$lib/types';

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
    console.log(get(this.state));
    return get(this.state).currentConversation;
  }

  async getOrCreate(conversationId: string | null): Promise<Conversation> {
    const conversation = await invoke<Conversation>('get_or_create_conversation', { conversationId });
    return conversation;
  }

  async getDisplayHistory(conversationId: string): Promise<Message[]> {
    const history = await invoke<DBMessage[]>('get_conversation_history', { conversationId });
    
    return history
      .sort((a, b) => {
        return (a.timestamp ?? 0) - (b.timestamp ?? 0);
      })
      .map(msg => ({
        type: msg.role === 'user' ? 'sent' : 'received',
        content: msg.content,
        attachments: msg.attachments
      }));
  }

  async getAPIHistory(conversationId: string): Promise<APIMessage[]> {
    const history = await invoke<DBMessage[]>('get_conversation_history', { conversationId });
    
    return history
      .sort((a, b) => {
        return (a.timestamp ?? 0) - (b.timestamp ?? 0);
      })
      .map(msg => ({
        role: msg.role,
        content: msg.content,
        attachments: msg.attachments
      }));
  }

  async saveMessage(
    role: 'user' | 'assistant', 
    content: string,
    attachments: Attachment[] = [],
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