# Takeaways from 05_02_ui

Patterns from the `05_02_ui` streaming chat demo worth porting into `ai-frontend`.
Ordered by effort vs impact.

---

## 1. rAF-based event batching *(one day, immediate perf win)*

**Problem**: `honoEventBridge.ts` calls `onEvent()` synchronously for every SSE chunk. Each call triggers a Svelte store write → reactive update. Under a fast `text_delta` stream this means hundreds of re-renders per second.

There's a partial fix: `agentOutputBuffer` + `agentOutputFlushTimer` in `chat.ts` throttles sub-agent output with `setTimeout`, but it only covers one event type.

**Fix**: wrap the `onEvent` call in a single rAF gate covering all event types.

```ts
// inside streamMessageViaHono (honoEventBridge.ts)
const pending: AgentEvent[] = []
let rafScheduled = false

const emit = (event: AgentEvent) => {
  pending.push(event)
  if (!rafScheduled) {
    rafScheduled = true
    requestAnimationFrame(() => {
      rafScheduled = false
      const batch = pending.splice(0)
      for (const e of batch) onEvent(e)
    })
  }
}

// replace all onEvent(...) calls with emit(...)
```

After this, delete `agentOutputFlushTimer` / `agentOutputBuffer` — they're redundant.

**Reference**: `src/lib/stores/chat-store.svelte.ts:224-245` in `05_02_ui`.

---

## 2. Incremental markdown segmentation *(medium, biggest win for long messages)*

**Problem**: `markdownCache.ts` caches `fullContent → renderedHTML` by the complete string. During streaming, every `text_delta` appends to the content, creating a new cache key — zero reuse across deltas. For a 5 000-character message arriving in 200 chunks, the parser runs 200 times on ever-growing strings.

`streamingSegmentedLength` in `chat.ts` is a partial stab at the idea but is a raw character-count rather than actual segment tracking.

**Fix**: port `parse-blocks.ts` + `streaming-markdown.ts` from `05_02_ui`.

Core idea:
- Split accumulated content into markdown blocks using the `marked` Lexer (respects code fences, HTML nesting, `$$` math, footnotes).
- All blocks except the **last** are *committed* — they have stable IDs, get rendered once, and are never re-parsed.
- Only the **last block** (the *live tail*) re-renders on every delta.

```ts
// syncIncrementalMarkdownView logic (simplified)
const blocks = parseMarkdownIntoBlocks(content)
const committedSegments = streaming ? blocks.slice(0, -1) : blocks
const liveTail = streaming ? (blocks.at(-1) ?? '') : ''
// committedSegments rendered with {#each} keyed by stable id → no re-render
// liveTail rendered separately, re-renders on every delta
```

Wire `streamingSegmentedLength` to the character-length boundary of the last committed segment rather than a manual counter — the store value becomes `committedSegments.reduce((n, s) => n + s.length, 0)`.

**Reference**: `src/lib/runtime/parse-blocks.ts` + `src/lib/runtime/streaming-markdown.ts` in `05_02_ui`.

---

## 3. Sequence numbers for general deduplication *(medium, removes fragile per-case logic)*

**Problem**: approvals can arrive via three separate SSE paths — the `approval` event, `agent_status.waitingFor`, and `done.waitingFor`. `honoEventBridge.ts` has `surfacedApprovalIds` to deduplicate them:

```ts
const surfacedApprovalIds = new Set<string>()
// ... if (!surfacedApprovalIds.has(entry.callId)) { ... } — in three places
```

This is a hand-rolled dedup for one event type. Any new event that can arrive via multiple paths needs its own set.

**Fix**: add a monotonic `seq` counter to server `AgentEvent`, track `lastSeq` per-message on the client, and drop any event where `seq <= lastSeq`.

Server side (`events/types.ts`):
```ts
interface BaseEvent {
  agent_id: string
  session_id: string
  timestamp: number
  seq: number  // add this — increment in AgentEventEmitter.emit()
}
```

Client side (`honoEventBridge.ts` or `chat.ts`):
```ts
let lastSeq = 0
// on every event: if (event.seq <= lastSeq) continue; lastSeq = event.seq
```

`surfacedApprovalIds` and all three dedup sites collapse into one guard.

**Reference**: `shared/chat.ts` BaseStreamEvent + `chat-store.svelte.ts:116-119` in `05_02_ui`.

---

## 4. Conversation snapshot endpoint for reconnection *(larger, reliability)*

**Problem**: if the SSE connection drops mid-stream the client loses that turn's state. There's no way to reconnect and catch up — the agent may have continued running on the server.

**Fix**: add a `GET /api/sessions/:id/events?after=<seq>` endpoint on the Hono server. The `items` table already stores everything needed. Return past items as a replay stream from a given sequence position. On reconnect, the client fetches from its last known `seq` and re-ingests missed events through the same `onEvent` path.

Reference pattern: `GET /api/conversation` in `05_02_ui/server/index.ts` returns a full `ConversationSnapshot` including all stored events for hydration.

---

## 5. Mock mode for development *(larger, DX)*

**Problem**: to see UI states like tool approval, thinking blocks, sub-agent execution, or workflow discussion you need a running Hono server with real API keys and a model that exercises those paths.

**Fix**: add a `mock` flag to `POST /api/chat/completions`. When set, the server plays back a pre-scripted sequence of `AgentEvent`s with configurable delays instead of calling any LLM. Scripts live as TypeScript fixtures (one per scenario: `tool-approval.ts`, `subagent.ts`, `workflow.ts`, `error.ts`).

```ts
// server/src/mock/scenarios.ts
export const toolApprovalScenario = [
  { type: 'text_delta', payload: { text: 'Let me check that for you...' }, delayMs: 300 },
  { type: 'tool:proposed', payload: { name: 'shell', callId: 'tc-1', args: { cmd: 'ls' } }, delayMs: 200 },
  // ... user approves via POST /api/chat/agents/:id/approve
  { type: 'tool:completed', payload: { callId: 'tc-1', success: true, output: 'file1.txt\nfile2.txt' }, delayMs: 500 },
  { type: 'text_delta', payload: { text: 'Found 2 files.' }, delayMs: 200 },
]
```

Reference: `server/mock/scenarios.ts` + `server/mock/builder.ts` in `05_02_ui`.
