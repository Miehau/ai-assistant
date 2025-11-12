/**
 * Minimal logger utility with a global DEBUG flag.
 *
 * Features:
 * - Toggle debug on/off globally (at runtime) via setDebug(true/false).
 * - Read initial state from:
 *   - window.__APP_DEBUG__ (boolean)
 *   - localStorage.DEBUG / localStorage.APP_DEBUG ("true", "1")
 *   - import.meta.env.VITE_DEBUG / import.meta.env.DEBUG ("true", true)
 * - Create namespaced loggers: createLogger('Chat'), createLogger('Chat:Messages'), etc.
 * - When DEBUG=false, debug/info are silenced; warn/error always pass through.
 *
 * Usage:
 *   import { createLogger, setDebug, isDebug } from '$lib/utils/logger';
 *   const log = createLogger('Chat');
 *   log.debug('hello', { data: 1 });
 *   log.warn('something suspicious');
 *   setDebug(true); // enable globally
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface Logger {
  debug: (...args: unknown[]) => void;
  info: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
  time: (label: string) => void;
  timeEnd: (label: string) => void;
}

declare global {
  interface Window {
    __APP_DEBUG__?: boolean;
  }
}

let debugEnabled = readInitialDebugFlag();
const timers = new Map<string, number>();

function safeReadLocalStorage(key: string): string | null {
  try {
    if (typeof localStorage === 'undefined') return null;
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function parseBooleanLike(val: unknown): boolean | null {
  if (typeof val === 'boolean') return val;
  if (typeof val === 'string') {
    const v = val.trim().toLowerCase();
    if (v === 'true' || v === '1' || v === 'yes' || v === 'on') return true;
    if (v === 'false' || v === '0' || v === 'no' || v === 'off') return false;
  }
  return null;
}

function readInitialDebugFlag(): boolean {
  // 1) Explicit runtime flag set before modules load
  if (typeof window !== 'undefined' && typeof window.__APP_DEBUG__ === 'boolean') {
    return window.__APP_DEBUG__;
  }

  // 2) localStorage override
  const ls = safeReadLocalStorage('DEBUG') ?? safeReadLocalStorage('APP_DEBUG');
  const parsedLs = parseBooleanLike(ls);
  if (parsedLs !== null) return parsedLs;

  // 3) Vite environment flags (frontend build-time)
  try {
    // @ts-ignore - import.meta.env exists in Vite environments
    const env = typeof import.meta !== 'undefined' ? import.meta.env : undefined;
    const envVal = env?.VITE_DEBUG ?? env?.DEBUG;
    const parsedEnv = parseBooleanLike(envVal);
    if (parsedEnv !== null) return parsedEnv;
  } catch {
    // ignore
  }

  // 4) Default: off
  return false;
}

export function setDebug(enabled: boolean): void {
  debugEnabled = !!enabled;
  if (typeof window !== 'undefined') {
    window.__APP_DEBUG__ = debugEnabled;
  }
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('DEBUG', String(debugEnabled));
    }
  } catch {
    // ignore storage errors
  }
}

export function isDebug(): boolean {
  return debugEnabled;
}

function prefix(namespace?: string): string[] {
  if (!namespace) return [];
  return [`[${namespace}]`];
}

function shouldLog(level: LogLevel): boolean {
  if (level === 'warn' || level === 'error') return true;
  return debugEnabled;
}

function logWithLevel(level: LogLevel, namespace?: string, ...args: unknown[]) {
  if (!shouldLog(level)) return;
  const p = prefix(namespace);

  // Route to the right console method; fallback to console.log
  const method =
    level === 'debug'
      ? console.debug ?? console.log
      : level === 'info'
      ? console.info ?? console.log
      : level === 'warn'
      ? console.warn ?? console.log
      : console.error ?? console.log;

  try {
    method.apply(console, [...p, ...args]);
  } catch {
    // Avoid crashing on weird console implementations
    // eslint-disable-next-line no-console
    console.log(...p, ...args);
  }
}

export function createLogger(namespace?: string): Logger {
  return {
    debug: (...args: unknown[]) => logWithLevel('debug', namespace, ...args),
    info: (...args: unknown[]) => logWithLevel('info', namespace, ...args),
    warn: (...args: unknown[]) => logWithLevel('warn', namespace, ...args),
    error: (...args: unknown[]) => logWithLevel('error', namespace, ...args),
    time: (label: string) => {
      if (!shouldLog('debug')) return;
      timers.set(namespacedLabel(namespace, label), performance.now());
    },
    timeEnd: (label: string) => {
      const key = namespacedLabel(namespace, label);
      const start = timers.get(key);
      if (start == null) return;
      timers.delete(key);
      const duration = performance.now() - start;
      logWithLevel('debug', namespace, `${label}: ${duration.toFixed(2)}ms`);
    }
  };
}

function namespacedLabel(namespace: string | undefined, label: string): string {
  return namespace ? `${namespace}::${label}` : label;
}

// Default unnamed logger
export const logger = createLogger();
