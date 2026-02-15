# Repo Map (Agent-Focused)

This is a short navigation map for fast orientation. Keep it updated when adding new core entrypoints.

Frontend entrypoints
- /Users/michalmlak/Projects/ai-frontend/src/App.svelte (app shell)
- /Users/michalmlak/Projects/ai-frontend/src/routes (page routes)
- /Users/michalmlak/Projects/ai-frontend/src/lib/services/chat.ts (send message + provider routing)
- /Users/michalmlak/Projects/ai-frontend/src/lib/services/eventBridge.ts (agent event stream bridge)
- /Users/michalmlak/Projects/ai-frontend/src/lib/stores (global state)
- /Users/michalmlak/Projects/ai-frontend/src/lib/models (model registry + selection)

Backend entrypoints
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/main.rs (Tauri app bootstrap)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/commands/agent.rs (agent IPC commands)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/agent/orchestrator.rs (controller loop)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/agent/prompts.rs (controller/responder prompts)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/tools (tool definitions + schemas)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/tool_outputs.rs (persisted tool output store)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/llm (provider implementations)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/db (SQLite models + operations)

Docs
- /Users/michalmlak/Projects/ai-frontend/src-tauri/docs/agent/README.md (agent docs index)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/docs/agent/provider-contracts.md (guardrails)
