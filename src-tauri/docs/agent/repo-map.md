# Repo Map (Agent-Focused)

This is a short navigation map for fast orientation. Keep it updated when adding new core entrypoints.

Frontend entrypoints
- src/App.svelte (app shell)
- src/routes (page routes)
- src/lib/services/chat.ts (send message + provider routing)
- src/lib/services/eventBridge.ts (agent event stream bridge)
- src/lib/stores (global state)
- src/lib/models (model registry + selection)

Backend entrypoints
- src-tauri/src/main.rs (Tauri app bootstrap)
- src-tauri/src/commands/agent.rs (agent IPC commands)
- src-tauri/src/agent/orchestrator.rs (controller loop)
- src-tauri/src/agent/prompts.rs (controller/responder prompts)
- src-tauri/src/tools (tool definitions + schemas)
- src-tauri/src/tool_outputs.rs (persisted tool output store)
- src-tauri/src/llm (provider implementations)
- src-tauri/src/db (SQLite models + operations)

Docs
- src-tauri/docs/agent/README.md (agent docs index)
- src-tauri/docs/agent/provider-contracts.md (guardrails)
