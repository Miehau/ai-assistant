# Chat Freeze & Visibility Work Plan

Owner: Frontend
Status: Planned → In Progress
Last updated: YYYY-MM-DD

## Summary

When chatting with AI, switching away (browser tab or in-app route) and returning to Chat causes the app to freeze or become unresponsive. Investigation identified a burst of reactive work and DOM operations on visibility changes and remounts, compounded by duplicate listeners and heavy parsing work.

This work plan outlines concrete, low-risk fixes and subsequent performance improvements to eliminate freezes and make re-entry into Chat smooth.

---

## Reproduction

- Start a conversation and stream a response.
- Switch to another browser tab (or navigate to another in-app section), then return to Chat mid/after-stream.
- Observe: UI jank, long main-thread stalls, sometimes “freeze” for 1–3+ seconds (varies with message count and attachment types).

---

## Root Causes and Contributing Factors

1) Duplicate scroll listener in `ChatMessages.svelte`
- The component adds a scroll handler both via Svelte markup (`on:scroll`) and imperatively (`addEventListener`), doubling the work on every scroll and on programmatic scrolls.

2) Re-running expensive loads on every Chat mount
- `Chat.svelte` calls `loadModels()` and `loadSystemPrompts()` on every mount, even when already loaded.

3) Multiple visibility listeners and desynchronized state
- `Chat.svelte` and `ChatMessages.svelte` each manage `visibilitychange` with separate flags and different debounces. On resume, they can trigger overlapping reactive cascades and DOM work.

4) ResizeObserver writes causing reflow storms
- The observer schedules scroll preservation close to resize, risking resize → write → resize feedback and long tasks.

5) Markdown + Prism re-rendering on remount
- Even with a cache, (re)mounting many `ChatMessage` nodes inserts large HTML chunks, causing style/layout work spikes.

6) Extremely verbose logging in hot paths
- Logging in reactive blocks, resize handlers, and RAF callbacks increases main-thread time during bursts.

7) Minor listener leak risk
- A deferred visibility handler in `FileAttachment.svelte` may not be removed if the component unmounts before the tab becomes visible.

---

## Goals

- Eliminate UI freezes and long main-thread stalls when returning to Chat.
- Ensure visibility changes do not trigger cascades across components.
- Improve perceived performance for large histories and rich content (code blocks, images).
- Keep behavior deterministic and testable.

## Non-Goals

- Large architectural changes (e.g., replacing the renderer entirely).
- Backend protocol changes.

---

## Plan of Record (Phased)

### Phase 1 — Low-risk, High-Impact Fixes

1) Remove duplicate scroll listener
- Keep `on:scroll={handleScroll}` in markup.
- Remove manual `addEventListener/removeEventListener` for scroll in `ChatMessages.svelte`.

2) Guard repeated model/prompt loading
- In `Chat.svelte`, skip `loadModels()` and `loadSystemPrompts()` when data is already present (or run them once at app bootstrap).
- Use `get()` or an “initialized” flag in the store.

3) Introduce a single debounced visibility store
- Add `src/lib/stores/visibility.ts` with one global `visibilitychange` listener and a small debounce (≈150–200ms).
- Replace local listeners in `Chat.svelte` and `ChatMessages.svelte` with the global store subscription (`$pageVisible`).
- Remove per-file visibility debounces to avoid timing mismatches.

4) Make ResizeObserver write-safe and minimal
- In `ChatMessages.svelte`, avoid immediate scrollTop writes in the same frame as resize.
- Use `requestAnimationFrame` (read first frame, write next frame).
- Skip resize work when page is not visible (`$pageVisible === false`).

5) Gate debug logs
- Wrap `console.log` calls in a simple `DEBUG` flag or a logger module.
- Reduce verbosity in reactive sections, ResizeObserver callbacks, and RAF callbacks.

6) Fix one-off visibility handler cleanup in `FileAttachment.svelte`
- Track the handler and remove it in `onDestroy` if still registered.

Deliverable: App is smooth when switching visibility, no perceived freezes on common flows.

### Phase 2 — Workload Reduction on Remount

7) Defer initial Markdown parsing
- In `ChatMessage.svelte`, do the initial `parseMarkdown` in `requestIdleCallback` (fallback to `setTimeout(0)`).
- Keep global parse cache to avoid redundant parsing.

8) Lazy-load Prism language packs (optional)
- Load only needed languages on-demand or reduce the set.
- This step is optional and can be evaluated after Phase 1.

9) Throttle streaming markdown updates further (optional)
- Maintain current separation of streaming content. Consider increasing the minimum delta or parse cadence if needed.

Deliverable: Lower initial mount costs, fewer stalls on conversations with many messages.

### Phase 3 — Virtualize Long Message Lists (optional but recommended)

10) Introduce list virtualization in Chat
- Replace naive render with virtualization (e.g., `svelte-virtual`).
- Only render visible messages and minimal overscan.

Deliverable: Scalability with large histories while maintaining smoothness.

### Phase 4 — QA/Regression

11) Manual QA and profiling
- Scenarios: streaming on/off, attachments (images/audio), large history (200+ messages), tab switches (browser and in-app).
- Validate no “ResizeObserver loop limit exceeded” warnings, and Performance profiler shows no long tasks on visibility return.

12) Rollout
- Keep the debug logger flag to enable quick field diagnostics.
- Stage and production checks.

---

## Implementation Checklist (Phase 1)

- [ ] ChatMessages: remove imperatively attached scroll handler (keep Svelte `on:scroll` only).
- [ ] Chat: guard `loadModels()` and `loadSystemPrompts()` (run only once or when needed).
- [ ] Add global `visibility.ts` store with debounced `pageVisible` boolean.
- [ ] Replace visibility listeners in `Chat.svelte` with `$pageVisible`.
- [ ] Replace visibility listeners in `ChatMessages.svelte` with `$pageVisible`.
- [ ] ChatMessages: make ResizeObserver callback write-safe via RAF; skip when not visible.
- [ ] Add `DEBUG` flag and gate logs in Chat*, ChatMessages*, ChatMessage*, and FileAttachment.
- [ ] FileAttachment: ensure deferred visibility handler is removed on destroy if still active.
- [ ] Smoke test: open/close Chat, visibility toggles, streaming, attachments.

Files to add:
- [ ] `src/lib/stores/visibility.ts` (new debounced `pageVisible` store)
- [ ] `src/lib/utils/logger.ts` (optional: simple logger with enable/disable)

Files to modify:
- [ ] `src/lib/components/chat/ChatMessages.svelte`
- [ ] `src/lib/components/Chat.svelte`
- [ ] `src/lib/components/ChatMessage.svelte` (Phase 2)
- [ ] `src/lib/components/files/FileAttachment.svelte`

---

## Acceptance Criteria

- Returning to the Chat view does not freeze or stall the UI, even with 100+ messages and active streaming.
- No “ResizeObserver loop limit exceeded” warnings when switching tabs/routes.
- Visibility toggling (browser tab or in-app route) triggers at most a single, controlled update per component (no cascades).
- Scrolling behavior remains correct (auto-scroll at bottom, preserved position on resize).
- Console logs are quiet by default; enabling a debug flag shows detailed logs when needed.

---

## Risk Assessment and Mitigations

- Behavior drift in scroll logic
  - Mitigation: Keep e2e checks for “auto-scroll at bottom when streaming” and “no forced scroll when user is scrolled up”.
- Visibility state desync
  - Mitigation: Single global source of truth for visibility; remove component-level listeners.
- Markdown render delays
  - Mitigation: Defer initial render but keep raw text fallback. Cache ensures already parsed content is instant later.

Rollback Plan:
- Each change is localized and can be reverted individually.
- Feature flags for logging and deferred parsing can be toggled off quickly.

---

## Task Breakdown (PR Plan)

PR-1: Phase 1 stabilization
- Remove duplicate scroll listener.
- Guard model/prompt loading in Chat mount.
- Introduce global visibility store and apply to Chat + ChatMessages.
- Make ResizeObserver write-safe and visibility-aware.
- Gate logs under `DEBUG`.
- Fix FileAttachment deferred listener cleanup.

PR-2: Phase 2 parsing improvements
- Defer initial Markdown parse via `requestIdleCallback`.
- Optional: narrow Prism language set or lazy-load.

PR-3 (optional): Virtualization
- Introduce `svelte-virtual` (or similar) to render only visible messages.

---

## Test Plan

Manual checks:
- Streaming: on/off, slow/fast models.
- Attachments: images (with thumbnails), audio, text files.
- Large history: 200+ messages (mix of code blocks and prose).
- Visibility changes: rapid tab switching, long background periods, frequent in-app navigation.
- Resize: window resize and container size changes while at bottom / scrolled up.

Instrumentation:
- Performance profiler before/after to ensure no long tasks on resume.
- Debug logs gated and confirm single visibility update path.

---

## Immediate Next Steps (Start Implementing)

- Create `src/lib/stores/visibility.ts` with a debounced `pageVisible` writable store sourced from `document.hidden`.
- Update `Chat.svelte` to consume `$pageVisible` and remove local visibility listener logic.
- Update `ChatMessages.svelte`:
  - Remove manual scroll event registration (keep `on:scroll`).
  - Replace visibility listener with `$pageVisible`.
  - Adjust ResizeObserver callback to schedule scroll preservation with RAF (read first frame, write next).
- Add a minimal `logger` utility with a `DEBUG` toggle; wrap hot-path logs.

Mark these items as “In Progress” in the checklist and open PR-1.

---