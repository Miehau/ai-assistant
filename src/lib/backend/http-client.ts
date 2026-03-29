/**
 * HTTP Backend Client — communicates with the standalone Hono server
 * via fetch() + SSE instead of Tauri IPC.
 *
 * Drop-in replacement for the Tauri BackendClient for the agent/chat
 * subset of the API. Tauri-only features (file versioning, image
 * processing, etc.) are not covered here.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Mirrors server domain types for the HTTP surface */

export type AgentStatus =
  | 'pending'
  | 'running'
  | 'waiting'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface WaitingFor {
  callId: string;
  type: 'tool' | 'approval' | 'agent' | 'human';
  name: string;
  args?: Record<string, unknown>;
  description?: string;
}

export interface Item {
  id?: string;
  agentId?: string;
  type: string;
  role?: string;
  content?: string | null;
  callId?: string | null;
  name?: string | null;
  arguments?: string | null;
  output?: string | null;
  isError?: boolean | null;
  turnNumber?: number;
}

export interface CompletionRequest {
  sessionId?: string;
  model?: string;
  agent?: string;
  input: string | Item[];
  instructions?: string;
  systemPrompt?: string;
  tools?: string[];
  stream?: boolean;
  temperature?: number;
  maxTokens?: number;
}

export interface CompletionResponse {
  id: string;
  sessionId: string;
  status: AgentStatus;
  output?: Item[];
  usage?: { turnCount: number };
  waitingFor?: WaitingFor[];
  error?: string;
}

export interface AgentStatusResponse {
  id: string;
  sessionId: string;
  parentId?: string | null;
  sourceCallId?: string | null;
  depth?: number;
  status: AgentStatus;
  waitingFor?: WaitingFor[];
  result?: string | null;
  error?: string | null;
  turnCount: number;
}

/** Mirrors server/src/domain/types.ts → Item */
export interface SessionItem {
  id: string;
  agentId: string;
  sequence: number;
  type: 'message' | 'function_call' | 'function_call_output' | 'reasoning';
  role: 'system' | 'user' | 'assistant' | null;
  content: string | null;
  callId: string | null;
  name: string | null;
  arguments: string | null;
  output: string | null;
  isError: boolean | null;
  durationMs: number | null;
  turnNumber: number;
  createdAt: number;
}

export interface Session {
  id: string;
  userId: string;
  rootAgentId: string | null;
  title: string | null;
  summary: string | null;
  status: 'active' | 'archived';
  createdAt: number;
  updatedAt: number;
  items?: SessionItem[];
  agents?: AgentStatusResponse[];
}

export interface ToolMetadata {
  name: string;
  description?: string;
  [key: string]: unknown;
}

export interface ModelInfo {
  id?: string;
  provider: string;
  name: string;
  displayName?: string;
  maxTokens?: number;
  contextWindow?: number;
  [key: string]: unknown;
}

export interface SSEEvent {
  event: string;
  data: Record<string, unknown>;
  id?: string;
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

export class HttpBackendError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly body?: unknown,
  ) {
    super(message);
    this.name = 'HttpBackendError';
  }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

export interface HttpClientConfig {
  serverUrl?: string;
  token?: string;
}

export class HttpBackendClient {
  private serverUrl: string;
  private token: string | null;

  constructor(config: HttpClientConfig = {}) {
    this.serverUrl = (config.serverUrl ?? 'http://localhost:3001').replace(
      /\/$/,
      '',
    );
    this.token = config.token ?? null;
  }

  // ---- Config helpers -----------------------------------------------------

  setServerUrl(url: string): void {
    this.serverUrl = url.replace(/\/$/, '');
  }

  setToken(token: string | null): void {
    this.token = token;
  }

  // ---- Internal helpers ---------------------------------------------------

  private headers(extra: Record<string, string> = {}): Record<string, string> {
    const h: Record<string, string> = {
      'Content-Type': 'application/json',
      ...extra,
    };
    if (this.token) {
      h['Authorization'] = `Bearer ${this.token}`;
    }
    return h;
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    signal?: AbortSignal,
  ): Promise<T> {
    const url = `${this.serverUrl}${path}`;
    const init: RequestInit = {
      method,
      headers: this.headers(),
      signal,
    };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }

    let res: Response;
    try {
      res = await fetch(url, init);
    } catch (err) {
      throw new HttpBackendError(
        `Network error: ${err instanceof Error ? err.message : String(err)}`,
        0,
      );
    }

    if (!res.ok) {
      let errorBody: unknown;
      try {
        errorBody = await res.json();
      } catch {
        errorBody = await res.text().catch(() => null);
      }
      const msg =
        errorBody && typeof errorBody === 'object' && 'error' in errorBody
          ? (errorBody as { error: string }).error
          : `HTTP ${res.status}`;
      throw new HttpBackendError(msg, res.status, errorBody);
    }

    try {
      return (await res.json()) as T;
    } catch {
      return undefined as unknown as T;
    }
  }

  // ========================================================================
  // Chat / Completions
  // ========================================================================

  /**
   * Non-streaming completion. Returns when the agent finishes or is waiting.
   */
  async sendMessage(
    input: string | Item[],
    options: Omit<CompletionRequest, 'input' | 'stream'> = {},
    signal?: AbortSignal,
  ): Promise<CompletionResponse> {
    return this.request<CompletionResponse>(
      'POST',
      '/api/chat/completions',
      { ...options, input, stream: false },
      signal,
    );
  }

  /**
   * Streaming completion via SSE.
   * Yields parsed SSE events as they arrive from the server.
   */
  async *sendMessageStream(
    input: string | Item[],
    options: Omit<CompletionRequest, 'input' | 'stream'> = {},
    signal?: AbortSignal,
  ): AsyncIterable<SSEEvent> {
    const url = `${this.serverUrl}/api/chat/completions`;
    let res: Response;

    try {
      res = await fetch(url, {
        method: 'POST',
        headers: this.headers(),
        body: JSON.stringify({ ...options, input, stream: true }),
        signal,
      });
    } catch (err) {
      throw new HttpBackendError(
        `Network error: ${err instanceof Error ? err.message : String(err)}`,
        0,
      );
    }

    if (!res.ok) {
      let errorBody: unknown;
      try {
        errorBody = await res.json();
      } catch {
        errorBody = null;
      }
      const msg =
        errorBody && typeof errorBody === 'object' && 'error' in errorBody
          ? (errorBody as { error: string }).error
          : `HTTP ${res.status}`;
      throw new HttpBackendError(msg, res.status, errorBody);
    }

    if (!res.body) {
      throw new HttpBackendError('Response body is null', 0);
    }

    yield* this.parseSSEStream(res.body);
  }

  // ========================================================================
  // Agent interactions
  // ========================================================================

  /**
   * Deliver a tool result to a waiting agent.
   */
  async deliverResult(
    agentId: string,
    callId: string,
    output: string,
    isError = false,
    signal?: AbortSignal,
  ): Promise<CompletionResponse> {
    return this.request<CompletionResponse>(
      'POST',
      `/api/chat/agents/${agentId}/deliver`,
      { callId, output, isError },
      signal,
    );
  }

  /**
   * Approve or deny a pending tool execution.
   */
  async approveToolExecution(
    agentId: string,
    callId: string,
    decision: 'approved' | 'denied',
    scope?: 'once' | 'conversation' | 'always',
    signal?: AbortSignal,
  ): Promise<CompletionResponse> {
    return this.request<CompletionResponse>(
      'POST',
      `/api/chat/agents/${agentId}/approve`,
      { callId, decision, scope },
      signal,
    );
  }

  /**
   * Cancel a running or waiting agent.
   */
  async cancelAgent(
    agentId: string,
    signal?: AbortSignal,
  ): Promise<{ id: string; status: string }> {
    return this.request<{ id: string; status: string }>(
      'POST',
      `/api/chat/agents/${agentId}/cancel`,
      undefined,
      signal,
    )
  }

  /**
   * Get the current status of an agent.
   */
  async getAgentStatus(
    agentId: string,
    signal?: AbortSignal,
  ): Promise<AgentStatusResponse> {
    return this.request<AgentStatusResponse>(
      'GET',
      `/api/chat/agents/${agentId}`,
      undefined,
      signal,
    );
  }

  /**
   * Subscribe to real-time events for an agent via SSE.
   * The returned async iterable yields events until the agent completes/fails
   * or the signal is aborted.
   */
  async *subscribeToEvents(
    agentId: string,
    signal?: AbortSignal,
  ): AsyncIterable<SSEEvent> {
    const url = `${this.serverUrl}/api/chat/agents/${agentId}/events`;
    let res: Response;

    try {
      res = await fetch(url, {
        method: 'GET',
        headers: {
          ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
          Accept: 'text/event-stream',
        },
        signal,
      });
    } catch (err) {
      throw new HttpBackendError(
        `Network error: ${err instanceof Error ? err.message : String(err)}`,
        0,
      );
    }

    if (!res.ok) {
      let errorBody: unknown;
      try {
        errorBody = await res.json();
      } catch {
        errorBody = null;
      }
      throw new HttpBackendError(
        `Failed to subscribe to events: HTTP ${res.status}`,
        res.status,
        errorBody,
      );
    }

    if (!res.body) {
      throw new HttpBackendError('Response body is null', 0);
    }

    yield* this.parseSSEStream(res.body);
  }

  // ========================================================================
  // Sessions
  // ========================================================================

  async listSessions(signal?: AbortSignal): Promise<Session[]> {
    return this.request<Session[]>('GET', '/api/sessions', undefined, signal);
  }

  async getSession(id: string, signal?: AbortSignal): Promise<Session> {
    return this.request<Session>(
      'GET',
      `/api/sessions/${id}`,
      undefined,
      signal,
    );
  }

  async updateSession(
    id: string,
    updates: { title?: string; status?: 'active' | 'archived' },
    signal?: AbortSignal,
  ): Promise<Session> {
    return this.request<Session>(
      'PATCH',
      `/api/sessions/${id}`,
      updates,
      signal,
    );
  }

  async deleteSession(
    id: string,
    signal?: AbortSignal,
  ): Promise<{ ok: boolean }> {
    return this.request<{ ok: boolean }>(
      'DELETE',
      `/api/sessions/${id}`,
      undefined,
      signal,
    );
  }

  // ========================================================================
  // Models
  // ========================================================================

  async listModels(signal?: AbortSignal): Promise<ModelInfo[]> {
    return this.request<ModelInfo[]>('GET', '/api/models', undefined, signal);
  }

  async addModel(
    provider: string,
    modelName: string,
    displayName?: string,
    contextWindow?: number,
    maxTokens?: number,
    signal?: AbortSignal,
  ): Promise<ModelInfo> {
    return this.request<ModelInfo>(
      'POST',
      '/api/models',
      { provider, modelName, displayName, contextWindow, maxTokens },
      signal,
    );
  }

  async deleteModel(id: string, signal?: AbortSignal): Promise<{ ok: boolean }> {
    return this.request<{ ok: boolean }>(
      'DELETE',
      `/api/models/${id}`,
      undefined,
      signal,
    );
  }

  // ========================================================================
  // Tools
  // ========================================================================

  async listTools(signal?: AbortSignal): Promise<ToolMetadata[]> {
    return this.request<ToolMetadata[]>('GET', '/api/tools', undefined, signal);
  }

  // ========================================================================
  // API Keys
  // ========================================================================

  async getApiKey(
    provider: string,
    signal?: AbortSignal,
  ): Promise<{ provider: string; exists: boolean; updatedAt: string | null }> {
    return this.request('GET', `/api/keys/${provider}`, undefined, signal);
  }

  async setApiKey(
    provider: string,
    apiKey: string,
    signal?: AbortSignal,
  ): Promise<{
    provider: string;
    exists: boolean;
    registered: boolean;
    updatedAt: string | null;
  }> {
    return this.request('PUT', `/api/keys/${provider}`, { apiKey }, signal);
  }

  async deleteApiKey(
    provider: string,
    signal?: AbortSignal,
  ): Promise<{ ok: boolean }> {
    return this.request('DELETE', `/api/keys/${provider}`, undefined, signal);
  }

  async listApiKeys(
    signal?: AbortSignal,
  ): Promise<{
    providers: Array<{
      provider: string;
      hasKey: boolean;
      isActive: boolean;
      updatedAt: string | null;
    }>;
  }> {
    return this.request('GET', '/api/keys', undefined, signal);
  }

  // ========================================================================
  // System Prompts
  // ========================================================================

  async listSystemPrompts(signal?: AbortSignal): Promise<unknown[]> {
    return this.request<unknown[]>('GET', '/api/system-prompts', undefined, signal);
  }

  async getSystemPrompt(id: string, signal?: AbortSignal): Promise<unknown> {
    return this.request('GET', `/api/system-prompts/${id}`, undefined, signal);
  }

  async createSystemPrompt(
    name: string,
    content: string,
    isDefault?: boolean,
    signal?: AbortSignal,
  ): Promise<unknown> {
    return this.request('POST', '/api/system-prompts', { name, content, isDefault }, signal);
  }

  async updateSystemPrompt(
    id: string,
    updates: { name?: string; content?: string; isDefault?: boolean },
    signal?: AbortSignal,
  ): Promise<unknown> {
    return this.request('PATCH', `/api/system-prompts/${id}`, updates, signal);
  }

  async deleteSystemPrompt(
    id: string,
    signal?: AbortSignal,
  ): Promise<{ ok: boolean }> {
    return this.request('DELETE', `/api/system-prompts/${id}`, undefined, signal);
  }

  // ========================================================================
  // Preferences
  // ========================================================================

  async getPreference(
    key: string,
    signal?: AbortSignal,
  ): Promise<{ key: string; value: string | null }> {
    return this.request('GET', `/api/preferences/${key}`, undefined, signal);
  }

  async setPreference(
    key: string,
    value: string,
    signal?: AbortSignal,
  ): Promise<{ key: string; value: string }> {
    return this.request('PUT', `/api/preferences/${key}`, { value }, signal);
  }

  // ========================================================================
  // Usage
  // ========================================================================

  async getUsageStats(
    signal?: AbortSignal,
  ): Promise<{ totalSessions: number; totalAgents: number; totalItems: number }> {
    return this.request('GET', '/api/usage', undefined, signal);
  }

  // ========================================================================
  // Models (additional)
  // ========================================================================

  async updateModel(
    id: string,
    updates: { displayName?: string; maxTokens?: number; contextWindow?: number },
    signal?: AbortSignal,
  ): Promise<ModelInfo> {
    return this.request<ModelInfo>('PATCH', `/api/models/${id}`, updates, signal);
  }

  // ========================================================================
  // SSE parsing
  // ========================================================================

  private async *parseSSEStream(
    body: ReadableStream<Uint8Array>,
  ): AsyncIterable<SSEEvent> {
    const reader = body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        // SSE events are separated by double newlines
        const parts = buffer.split('\n\n');
        // Keep the last (potentially incomplete) chunk in the buffer
        buffer = parts.pop() ?? '';

        for (const part of parts) {
          const event = this.parseSSEBlock(part);
          if (event) {
            yield event;

            // Stop on terminal events
            if (event.event === 'done' || event.event === 'error') {
              return;
            }
          }
        }
      }

      // Process any remaining data in the buffer
      if (buffer.trim()) {
        const event = this.parseSSEBlock(buffer);
        if (event) {
          yield event;
        }
      }
    } finally {
      reader.releaseLock();
    }
  }

  private parseSSEBlock(block: string): SSEEvent | null {
    const lines = block.split('\n');
    let eventName = 'message';
    let data = '';
    let id: string | undefined;

    for (const line of lines) {
      if (line.startsWith('event:')) {
        eventName = line.slice(6).trim();
      } else if (line.startsWith('data:')) {
        data += line.slice(5).trim();
      } else if (line.startsWith('id:')) {
        id = line.slice(3).trim();
      }
      // Lines starting with ':' are comments, skip them
    }

    if (!data) return null;

    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(data);
    } catch {
      parsed = { raw: data };
    }

    return { event: eventName, data: parsed, id };
  }
}

// ---------------------------------------------------------------------------
// Factory / singleton
// ---------------------------------------------------------------------------

let _instance: HttpBackendClient | null = null;

/**
 * Get or create the shared HTTP backend client.
 * Call with config to (re-)initialize; call without args to get the existing
 * instance.
 */
export function getHttpBackend(
  config?: HttpClientConfig,
): HttpBackendClient {
  if (config || !_instance) {
    _instance = new HttpBackendClient(config);
  }
  return _instance;
}
