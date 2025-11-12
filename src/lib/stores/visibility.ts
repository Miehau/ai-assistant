/**
 * Global, debounced page visibility store.
 *
 * Purpose:
 * - Provide a single source of truth for whether the page is visible.
 * - Debounce visibility changes to avoid reactive cascades when switching tabs/routes.
 * - Deduplicate component-level listeners – subscribe to this store instead.
 *
 * Usage:
 *   import { pageVisible } from '$lib/stores/visibility';
 *   $: if ($pageVisible) { /* do visible-only work */ /* }
 */

import { writable, type Subscriber, type Unsubscriber } from 'svelte/store';

const DEFAULT_DEBOUNCE_MS = 200;

const isBrowser =
  typeof window !== 'undefined' &&
  typeof document !== 'undefined' &&
  typeof document.addEventListener === 'function';

const initialVisible = isBrowser ? !document.hidden : true;

function createPageVisibilityStore(debounceMs = DEFAULT_DEBOUNCE_MS) {
  const { subscribe: baseSubscribe, set } = writable<boolean>(initialVisible);

  let subscribers = 0;
  let started = false;
  let timeoutId: number | null = null;

  const clearDebounce = () => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
  };

  const scheduleUpdate = (override?: boolean) => {
    if (!isBrowser) return;

    const next = typeof override === 'boolean' ? override : !document.hidden;

    clearDebounce();
    // Debounce to smooth out cascade on visibility return
    timeoutId = window.setTimeout(() => {
      set(next);
      timeoutId = null;
    }, debounceMs) as unknown as number;
  };

  const handleVisibilityChange = () => {
    // document.hidden is the canonical source of truth
    scheduleUpdate();
  };

  const handlePageShow = () => {
    // BFCache restore or visibility return – ensure we flip to true
    scheduleUpdate(true);
  };

  const handlePageHide = () => {
    // Page is being hidden (may be persisted in BFCache)
    scheduleUpdate(false);
  };

  const start = () => {
    if (started || !isBrowser) return;
    started = true;

    // Set the current state immediately
    set(!document.hidden);

    document.addEventListener('visibilitychange', handleVisibilityChange, true);
    // pageshow/pagehide handle BFCache restores and lifecycle edge-cases
    window.addEventListener('pageshow', handlePageShow, true);
    window.addEventListener('pagehide', handlePageHide, true);
  };

  const stop = () => {
    if (!started || !isBrowser) return;
    started = false;

    document.removeEventListener('visibilitychange', handleVisibilityChange, true);
    window.removeEventListener('pageshow', handlePageShow, true);
    window.removeEventListener('pagehide', handlePageHide, true);

    clearDebounce();
  };

  // Custom subscribe that starts listeners on first subscriber and stops on last
  const subscribe = (run: Subscriber<boolean>): Unsubscriber => {
    subscribers += 1;
    if (subscribers === 1) start();

    const unsubscribeBase = baseSubscribe(run);

    return () => {
      unsubscribeBase();
      subscribers -= 1;
      if (subscribers === 0) stop();
    };
  };

  return {
    subscribe,
    /**
     * Force an immediate refresh from the current document.hidden state.
     * Mostly useful for tests.
     */
    refresh: () => {
      if (!isBrowser) return;
      clearDebounce();
      set(!document.hidden);
    }
  };
}

/**
 * Exported debounced page visibility store.
 * True when the page is visible, false when hidden.
 */
export const pageVisible = createPageVisibilityStore();
