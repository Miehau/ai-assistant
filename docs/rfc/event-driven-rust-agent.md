# RFC: Event-Driven Rust Agent Orchestrator

**Status:** Draft  
**Date:** 2025-02-10  
**Owner:** TBD  
**Scope:** Move runtime AI logic from frontend (Svelte/TS) to Rust, with an event-driven backend and a thin UI.

---

## Summary

This RFC proposes a full migration of AI/agent orchestration, provider calls, and runtime logic from the frontend into the Rust backend. The frontend becomes a render-only client that issues high-level commands and consumes backend events. The Rust side becomes the system of record for message flow, state, and tooling.

---

## Goals

- Make Rust the single source of truth for chat/agent orchestration.
- Implement an internal Rust event bus and forward selected events to the UI.
- Keep the frontend thin: render state, capture input, and display streaming output.
- Preserve existing features (chat, branching, usage, attachments, title generation).
- Enable deterministic replay and inspection of agent behavior.

## Non-Goals

- UI redesigns or new UX features.
- New agent capabilities beyond current scope (unless required by migration).
- Cloud services or remote state.

---

## Current State (Condensed)

Frontend owns most runtime logic:

- Chat orchestration, streaming, branching, usage tracking, and title generation.
- Provider integrations (OpenAI/Anthropic/DeepSeek/custom).
- Model registry selection logic.

Rust handles:

- Database, attachments, files, and basic commands.

There is no backend event bus. Tauri events are not used for application-level state.

---

## Proposed Architecture

### 1) Rust Event Bus (Internal)

- Use a Rust channel-based event bus (e.g., `tokio::sync::broadcast`).
- The bus carries structured events for agent lifecycle and message streaming.
- One bridge component forwards selected events to the UI via Tauri emits.

### 2) Rust Agent Orchestrator

Core responsibilities:

- Model selection and provider routing.
- Prompt assembly and system prompts.
- Streaming response handling and buffering.
- Tool execution and agent iteration tracking.
- Usage tracking and cost calculation.
- Branch and conversation state updates.

### 3) Provider Clients in Rust

- Implement OpenAI/Anthropic/DeepSeek/custom backend clients using `reqwest`.
- Handle streaming SSE parsing in Rust.
- Standardize response types.

### 4) Event Schema

Backend emits typed events to the UI:

- `conversation.created`
- `message.user.received`
- `assistant.stream.started`
- `assistant.stream.chunk`
- `assistant.stream.completed`
- `agent.thinking.started`
- `agent.thinking.step`
- `tool.execution.started`
- `tool.execution.completed`
- `usage.updated`
- `conversation.title.updated`

Event payloads should include correlation IDs:
- `conversation_id`
- `message_id`
- `parent_message_id`
- `branch_id`
- `timestamp`

### 5) Frontend Responsibilities

- Capture user input and file selection.
- Send high-level commands (e.g., `send_message`).
- Subscribe to backend events and update UI state.
- Keep local UI-only stores (scroll state, visibility, etc).

---

## Data Model Changes

Use existing tables where possible:
- `messages`, `message_tree`, `message_tool_executions`, `message_agent_thinking`

Potential additions:
- `agent_events` (optional) for durable event logs and replay.
- `agent_state` (optional) for per-conversation status snapshots.

---

## API Surface (Tauri Commands)

New or revised commands:

- `agent_send_message(payload)` -> returns `message_id` and starts event stream
- `agent_cancel(message_id)`
- `agent_get_status(conversation_id)`
- `agent_get_history(conversation_id)` (optional, if frontend stops reading DB directly)

Frontend no longer calls provider APIs directly.

---

## Migration Plan (Phased)

### Phase 0: Baseline + Guardrails

- Add tracing/logging around existing calls.
- Freeze functionality changes during refactor.
- Capture current flows and feature parity requirements.

### Phase 1: Event Bus + Tauri Bridge

- Implement internal event bus in Rust.
- Add Tauri event bridge and minimal event schema.
- Emit events for existing DB operations (no provider migration yet).

### Phase 2: Rust Streaming for One Provider

- Move OpenAI streaming into Rust first.
- Frontend subscribes to stream events for assistant output.
- Feature-flagged fallback to old frontend path.

### Phase 3: Provider Migration

- Move Anthropic and DeepSeek into Rust.
- Move custom backend adapter into Rust.
- Remove provider logic from frontend.

### Phase 4: Orchestration Migration

- Move prompt formatting and model selection to Rust.
- Move title generation to Rust.
- Move usage tracking entirely to Rust.

### Phase 5: Frontend Slimming + Cleanup

- Remove frontend services: `chat.ts`, provider files, and orchestration logic.
- Replace with event-driven stores.
- Remove unused config and dependencies.

---

## Risks and Mitigations

- **Streaming complexity**: Use robust SSE parsing in Rust and test with real providers.
- **Event ordering**: Include sequence numbers and timestamps in event payloads.
- **State divergence**: Rust becomes source of truth; frontend only reflects events.
- **Performance**: Use buffered streaming and throttled UI updates.
- **Migration regressions**: Use feature flags and staged rollout.

---

## Acceptance Criteria

- No provider SDKs or LLM calls in frontend.
- All chat/agent operations initiated via Rust commands.
- UI receives streamed assistant output via events.
- DB updated only by Rust for conversations/messages/usage.
- Feature parity for core chat, branching, attachments, and title generation.

---

## Open Questions

- Where to host the model registry JSON (move to Rust or keep in frontend)?
- Should the backend event log be durable and replayable?
- How to handle partial streaming content if UI disconnects?

