/**
 * Backend module — auto-selects Tauri IPC or HTTP client based on runtime.
 *
 * Detection:
 *   - If `window.__TAURI__` exists  -> use Tauri IPC (BackendClient)
 *   - Otherwise                     -> use HTTP fetch  (HttpBackendClient)
 *
 * Usage:
 * ```typescript
 * import { backend, isHttpBackend } from '$lib/backend';
 * ```
 *
 * The `backend` export is the Tauri BackendClient singleton when running
 * inside Tauri. For the HTTP path you should import `httpBackend` directly
 * since the HTTP client has a different (chat-focused) API surface.
 */

export { backend, BackendClient } from './client';
export {
  HttpBackendClient,
  HttpBackendError,
  getHttpBackend,
  type HttpClientConfig,
  type CompletionRequest,
  type CompletionResponse,
  type AgentStatusResponse,
  type SSEEvent,
  type Session as HttpSession,
  type ModelInfo,
  type ToolMetadata as HttpToolMetadata,
} from './http-client';

/**
 * True when running outside Tauri (browser, Electron, etc.).
 * Use this to conditionally switch UI code paths.
 */
export function isHttpBackend(): boolean {
  return typeof window === 'undefined' || !(window as any).__TAURI__;
}

/**
 * Convenience: get the HTTP backend singleton (creates with defaults if needed).
 * Shorthand so callers don't need to import getHttpBackend separately.
 */
export { getHttpBackend as httpBackend } from './http-client';
