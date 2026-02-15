# Golden Flows

Keep these paths accurate; they are the shortest path to debugging regressions.

Message send to agent
- /Users/michalmlak/Projects/ai-frontend/src/lib/services/chat.ts (frontend entrypoint)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/commands/agent.rs (agent_send_message)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/agent/orchestrator.rs (controller loop)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/tools (tool execution)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/events.rs (event bus)
- /Users/michalmlak/Projects/ai-frontend/src/lib/services/eventBridge.ts (frontend listener)

Tool output persistence
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/tool_outputs.rs (store)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/tools/tool_outputs.rs (tool surface)

Model/provider selection
- /Users/michalmlak/Projects/ai-frontend/src/lib/models/modelService.ts (frontend model registry)
- /Users/michalmlak/Projects/ai-frontend/src/lib/services (provider services)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/llm (backend providers)

Agent trace (debug only)
- /Users/michalmlak/Projects/ai-frontend/src-tauri/src/commands/agent.rs (trace recording)
- /Users/michalmlak/Projects/ai-frontend/src/lib/services/agentTrace.ts (frontend fetch)
