# Rust Event-Driven Refactor Tracker

**Goal:** Move runtime AI/agent logic from frontend to Rust with event-driven architecture.  
**Status:** Not started  
**Owner:** TBD  
**Start Date:** TBD  

---

## Milestones

- [ ] M0: Baseline and feature parity checklist agreed
- [ ] M1: Rust event bus + Tauri bridge in place
- [ ] M2: OpenAI streaming in Rust (feature-flagged)
- [ ] M3: All providers migrated to Rust
- [ ] M4: Orchestration and title generation in Rust
- [ ] M5: Frontend logic removed and cleanup complete

---

## Phase 0: Baseline + Guardrails

- [ ] Document current chat flow (send, stream, save, branch, usage)
- [ ] Identify frontend-owned logic to migrate
- [ ] Define regression test checklist (manual or automated)
- [ ] Add temporary feature flag for provider selection (frontend vs Rust)

---

## Phase 1: Event Bus + UI Bridge

- [ ] Implement Rust event bus (broadcast or mpsc)
- [ ] Define event schema and payload types
- [ ] Emit events for message save/update
- [ ] Add UI event listeners and adapters (no logic changes yet)
- [ ] Write minimal event replay/testing harness

---

## Phase 2: OpenAI in Rust (Pilot)

- [ ] Add Rust OpenAI client with streaming
- [ ] Support cancel/abort flow
- [ ] Emit `assistant.stream.*` events
- [ ] Feature flag to switch frontend/provider path
- [ ] Validate usage tracking and DB writes

---

## Phase 3: Provider Migration

- [ ] Move Anthropic client to Rust
- [ ] Move DeepSeek client to Rust
- [ ] Move custom backend adapter to Rust
- [ ] Remove provider API calls from frontend

---

## Phase 4: Orchestration Migration

- [ ] Move prompt formatting to Rust
- [ ] Move model selection + registry handling to Rust
- [ ] Move usage tracking fully to Rust
- [ ] Move title generation to Rust
- [ ] Emit `agent.thinking.*` and `tool.execution.*` events

---

## Phase 5: Frontend Slimming + Cleanup

- [ ] Replace Svelte stores with event-driven state adapters
- [ ] Remove `src/lib/services/chat.ts`
- [ ] Remove `src/lib/services/openai.ts`
- [ ] Remove `src/lib/services/anthropic.ts`
- [ ] Remove `src/lib/services/deepseek.ts`
- [ ] Remove `src/lib/services/customProvider.ts`
- [ ] Remove unused config and registry code
- [ ] Update docs and onboarding notes

---

## Risks / Notes

- Ordering issues in streamed events
- UI state desync if events are dropped
- Provider rate limits and retry policy differences between TS and Rust
- Testing strategy for streaming and cancellation

