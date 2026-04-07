# Golden Flows

Keep these paths accurate; they are the shortest path to debugging regressions.

Message send to agent
- src/lib/services/chat.ts (frontend entrypoint)
- src-tauri/src/commands/agent.rs (agent_send_message)
- src-tauri/src/agent/orchestrator.rs (controller loop)
- src-tauri/src/tools (tool execution)
- src-tauri/src/events.rs (event bus)
- src/lib/services/eventBridge.ts (frontend listener)

Tool output persistence
- src-tauri/src/tool_outputs.rs (store)
- src-tauri/src/tools/tool_outputs.rs (tool surface)

Model/provider selection
- src/lib/models/modelService.ts (frontend model registry)
- src/lib/services (provider services)
- src-tauri/src/llm (backend providers)

Agent trace (debug only)
- src-tauri/src/commands/agent.rs (trace recording)
- src/lib/services/agentTrace.ts (frontend fetch)
