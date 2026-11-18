import { apiRaw } from './api';

export interface Conversation {
  id: string;
  platform: string;
  name: string;
  avatar?: string;
  last_message?: string;
  last_sent_at?: string;
  unread_count: number;
  is_group: boolean;
  member_count: number;
}

export interface ChatMessage {
  id: string;
  sender_id: string;
  sender_name: string;
  sender_avatar?: string;
  content: string;
  sent_at: string;
  platform: string;
}

export const chatApi = {
  async conversations(): Promise<Conversation[]> {
    const res = await apiRaw('/api/chat/conversations');
    if (!res.ok) throw new Error('Failed to fetch conversations');
    return res.json();
  },

  async messages(convoId: string, limit = 50, offset = 0): Promise<ChatMessage[]> {
    const res = await apiRaw(`/api/chat/conversations/${convoId}/messages?limit=${limit}&offset=${offset}`);
    if (!res.ok) throw new Error('Failed to fetch messages');
    return res.json();
  },

  async sendMessage(convoId: string, text: string): Promise<ChatMessage> {
    const res = await apiRaw(`/api/chat/conversations/${convoId}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text }),
    });
    if (!res.ok) throw new Error('Failed to send message');
    return res.json();
  },

  async startConversation(handle: string): Promise<Conversation> {
    const res = await apiRaw('/api/chat/conversations', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ handle }),
    });
    if (!res.ok) throw new Error('Failed to start conversation');
    return res.json();
  },
};