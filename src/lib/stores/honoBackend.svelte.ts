import { invoke } from '@tauri-apps/api/tauri';
import { getHttpBackend } from '$lib/backend/http-client';

const PREF_HONO_ENABLED = 'hono.enabled';
const PREF_HONO_URL = 'hono.server_url';
const PREF_HONO_TOKEN = 'hono.token';
const PREF_HONO_SESSION_MAP = 'hono.session_map';

class HonoBackendStore {
  enabled = $state(false);
  serverUrl = $state('http://localhost:3001');
  token = $state('');

  /** Maps local conversation_id → Hono session_id */
  private sessionMap = new Map<string, string>();

  async init() {
    try {
      const [enabled, url, token, sessionMapRaw] = await Promise.all([
        invoke<string | null>('get_preference', { key: PREF_HONO_ENABLED }),
        invoke<string | null>('get_preference', { key: PREF_HONO_URL }),
        invoke<string | null>('get_preference', { key: PREF_HONO_TOKEN }),
        invoke<string | null>('get_preference', { key: PREF_HONO_SESSION_MAP }),
      ]);
      this.enabled = enabled === 'true';
      this.serverUrl = url ?? 'http://localhost:3001';
      this.token = token ?? '';
      if (sessionMapRaw) {
        try {
          const parsed = JSON.parse(sessionMapRaw) as Record<string, string>;
          this.sessionMap = new Map(Object.entries(parsed));
        } catch { /* corrupt pref — start fresh */ }
      }
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
    this.persistSessionMap();
  }

  removeSession(conversationId: string) {
    this.sessionMap.delete(conversationId);
    this.persistSessionMap();
  }

  private persistSessionMap() {
    const obj = Object.fromEntries(this.sessionMap);
    invoke('set_preference', { key: PREF_HONO_SESSION_MAP, value: JSON.stringify(obj) })
      .catch((e) => console.error('[honoBackend] failed to persist session map', e));
  }

  getClient() {
    return getHttpBackend();
  }
}

export const honoBackend = new HonoBackendStore();
