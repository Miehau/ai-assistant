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
      .map(msg => {
        let content = msg.content;

        return {
          type: msg.role === 'user' ? 'sent' : 'received',
          content,
          attachments: msg.attachments
        };
      });
  }

  async getAPIHistory(conversationId: string): Promise<APIMessage[]> {
    const history = await invoke<DBMessage[]>('get_conversation_history', { conversationId });
    
    return history
      .sort((a, b) => {
        return (a.timestamp ?? 0) - (b.timestamp ?? 0);
      })
      .map(msg => ({
        role: msg.role,
        content: msg.content
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
  
  async updateConversationName(conversationId: string, name: string): Promise<void> {
    console.log('Calling update_conversation_name with:', { conversationId, name });
    try {
      await invoke('update_conversation_name', { conversationId, name });
      console.log('Backend update_conversation_name completed successfully');
      
      // If this is the current conversation, update the local state
      const currentState = get(this.state);
      if (currentState.currentConversationId === conversationId && currentState.currentConversation) {
        console.log('Updating local state with new conversation name');
        this.state.update(state => ({
          ...state,
          currentConversation: {
            ...state.currentConversation!,
            name
          }
        }));
        console.log('Local state updated successfully');
      } else {
        console.log('Not updating local state - not the current conversation');
      }
    } catch (error) {
      console.error('Error in updateConversationName:', error);
      throw error;
    }
  }

  async deleteConversation(conversationId: string): Promise<void> {
    console.log('Calling delete_conversation with:', { conversationId });
    try {
      await invoke('delete_conversation', { conversationId });
      console.log('Backend delete_conversation completed successfully');
      
      // If this is the current conversation, clear the current conversation
      const currentState = get(this.state);
      if (currentState.currentConversationId === conversationId) {
        console.log('Clearing current conversation state');
        this.state.update(state => ({
          ...state,
          currentConversationId: null,
          currentConversation: null
        }));
      }
    } catch (error) {
      console.error('Error in deleteConversation:', error);
      throw error;
    }
  }
}

export const conversationService = new ConversationService();