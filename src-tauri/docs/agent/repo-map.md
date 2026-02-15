# Repo Map (Agent-Focused)

This is a short navigation map for fast orientation. Keep it updated when adding new core entrypoints.

Frontend entrypoints
- ***REMOVED***Projects/ai-frontend/src/App.svelte (app shell)
- ***REMOVED***Projects/ai-frontend/src/routes (page routes)
- ***REMOVED***Projects/ai-frontend/src/lib/services/chat.ts (send message + provider routing)
- ***REMOVED***Projects/ai-frontend/src/lib/services/eventBridge.ts (agent event stream bridge)
- ***REMOVED***Projects/ai-frontend/src/lib/stores (global state)
- ***REMOVED***Projects/ai-frontend/src/lib/models (model registry + selection)

Backend entrypoints
- ***REMOVED***Projects/ai-frontend/src-tauri/src/main.rs (Tauri app bootstrap)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/commands/agent.rs (agent IPC commands)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/agent/orchestrator.rs (controller loop)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/agent/prompts.rs (controller/responder prompts)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/tools (tool definitions + schemas)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/tool_outputs.rs (persisted tool output store)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/llm (provider implementations)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/db (SQLite models + operations)

Docs
- ***REMOVED***Projects/ai-frontend/src-tauri/docs/agent/README.md (agent docs index)
- ***REMOVED***Projects/ai-frontend/src-tauri/docs/agent/provider-contracts.md (guardrails)
