import { invoke } from '@tauri-apps/api/tauri';
import { getHttpBackend } from '$lib/backend/http-client';

const PREF_HONO_ENABLED = 'hono.enabled';
const PREF_HONO_URL = 'hono.server_url';
const PREF_HONO_TOKEN = 'hono.token';

class HonoBackendStore {
  enabled = $state(false);
  serverUrl = $state('http://localhost:3001');
  token = $state('');

  /** Maps local conversation_id → Hono session_id */
  private sessionMap = new Map<string, string>();

  async init() {
    try {
      const [enabled, url, token] = await Promise.all([
        invoke<string | null>('get_preference', { key: PREF_HONO_ENABLED }),
        invoke<string | null>('get_preference', { key: PREF_HONO_URL }),
        invoke<string | null>('get_preference', { key: PREF_HONO_TOKEN }),
      ]);
      this.enabled = enabled === 'true';
      this.serverUrl = url ?? 'http://localhost:3001';
      this.token = token ?? '';
      if (this.enabled) {
        getHttpBackend({ serverUrl: this.serverUrl, token: this.token || undefined });
      }
    } catch (e) {
      console.error('[honoBackend] init failed', e);
    }
  }

  async save() {
    await Promise.all([
      invoke('set_preference', { key: PREF_HONO_ENABLED, value: String(this.enabled) }),
      invoke('set_preference', { key: PREF_HONO_URL, value: this.serverUrl }),
      invoke('set_preference', { key: PREF_HONO_TOKEN, value: this.token }),
    ]);
    getHttpBackend({ serverUrl: this.serverUrl, token: this.token || undefined });
  }

  getSessionId(conversationId: string): string | undefined {
    return this.sessionMap.get(conversationId);
  }

  setSessionId(conversationId: string, sessionId: string) {
    this.sessionMap.set(conversationId, sessionId);
  }

  getClient() {
    return getHttpBackend();
  }
}

export const honoBackend = new HonoBackendStore();
