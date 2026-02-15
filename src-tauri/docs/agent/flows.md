# Golden Flows

Keep these paths accurate; they are the shortest path to debugging regressions.

Message send to agent
- ***REMOVED***Projects/ai-frontend/src/lib/services/chat.ts (frontend entrypoint)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/commands/agent.rs (agent_send_message)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/agent/orchestrator.rs (controller loop)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/tools (tool execution)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/events.rs (event bus)
- ***REMOVED***Projects/ai-frontend/src/lib/services/eventBridge.ts (frontend listener)

Tool output persistence
- ***REMOVED***Projects/ai-frontend/src-tauri/src/tool_outputs.rs (store)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/tools/tool_outputs.rs (tool surface)

Model/provider selection
- ***REMOVED***Projects/ai-frontend/src/lib/models/modelService.ts (frontend model registry)
- ***REMOVED***Projects/ai-frontend/src/lib/services (provider services)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/llm (backend providers)

Agent trace (debug only)
- ***REMOVED***Projects/ai-frontend/src-tauri/src/commands/agent.rs (trace recording)
- ***REMOVED***Projects/ai-frontend/src/lib/services/agentTrace.ts (frontend fetch)
