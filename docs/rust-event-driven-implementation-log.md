# Rust Event-Driven Refactor Implementation Log

**Purpose:** Track completed implementation steps for the Rust-first, event-driven migration.  
**Started:** 2025-02-10  

---

## 2025-02-10

- Added a Rust `EventBus` scaffold with publish/subscribe support. (`src-tauri/src/events.rs`)
- Wired the event bus into Tauri setup and bridged events to the UI via `emit_all("agent_event")`. (`src-tauri/src/main.rs`)
- Defined an initial event schema and type exports in TypeScript. (`src/lib/types/events.ts`, `src/lib/types.ts`)
- Emitted a `message.saved` event from the Rust `save_message` command. (`src-tauri/src/commands/conversations.rs`, `src-tauri/src/events.rs`)
- Expanded `message.saved` payload to include content, attachments, and timestamp. (`src-tauri/src/commands/conversations.rs`, `src/lib/types/events.ts`)
- Added a frontend event bridge and chat store listener for `agent_event` messages. (`src/lib/services/eventBridge.ts`, `src/lib/stores/chat.ts`, `src/lib/components/Chat.svelte`)
- Added events for conversation updates/deletes and usage updates, plus frontend handling for usage updates. (`src-tauri/src/events.rs`, `src-tauri/src/commands/conversations.rs`, `src-tauri/src/commands/usage.rs`, `src/lib/types/events.ts`, `src/lib/stores/chat.ts`)
- Applied conversation update/delete events to in-memory state and chat UI. (`src/lib/services/conversation.ts`, `src/lib/stores/chat.ts`)
- Added assistant streaming event types and UI handlers (awaiting Rust emission). (`src-tauri/src/events.rs`, `src/lib/types/events.ts`, `src/lib/stores/chat.ts`)
- Added Rust LLM streaming scaffolding with reqwest, plus new agent command for event-driven chat. (`src-tauri/src/llm/mod.rs`, `src-tauri/src/commands/agent.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`)
- Switched chat send flow to invoke the Rust agent and rely on streaming events for UI updates. (`src/lib/stores/chat.ts`)
- Added branch tree updates for agent-driven messages and stream completion handling. (`src-tauri/src/commands/agent.rs`, `src/lib/stores/chat.ts`)
- Implemented provider streaming via reqwest (OpenAI, Anthropic, DeepSeek, custom/OpenAI-compatible, Ollama default). (`src-tauri/src/llm/mod.rs`, `src-tauri/src/commands/agent.rs`)
- Added non-streaming completion paths tied to the UI streaming toggle. (`src-tauri/src/llm/mod.rs`, `src-tauri/src/commands/agent.rs`, `src/lib/stores/chat.ts`)
- Synced loading state to assistant stream start/complete events. (`src/lib/stores/chat.ts`)
- Reset streaming/loading state on conversation deletion events. (`src/lib/stores/chat.ts`)
- Moved title generation to Rust via `agent_generate_title` and updated frontend to call the backend. (`src-tauri/src/commands/agent.rs`, `src-tauri/src/main.rs`, `src/lib/services/titleGenerator.ts`)
- Added basic attachment inclusion for text and images in Rust prompt assembly. (`src-tauri/src/commands/agent.rs`, `src-tauri/src/llm/mod.rs`)
- Fixed Rust agent streaming for custom/ollama providers and cleaned unused imports. (`src-tauri/src/commands/agent.rs`, `src-tauri/src/commands/file_versioning.rs`, `src-tauri/src/files/image.rs`)
- Throttled streaming updates via animation frame buffering to reduce UI lag and improve auto-scroll. (`src/lib/stores/chat.ts`)
- Disabled markdown parsing during streaming to avoid re-render churn. (`src/lib/components/ChatMessage.svelte`, `src/lib/components/chat/ChatMessages.svelte`)
