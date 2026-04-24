# Research Agent Unified Path Plan

## Summary

This document captures the agreed architecture for a research-focused agent flow that works from both the local app chat and Telegram.

Core goals:

- Use one shared backend execution path for local chat and Telegram.
- Keep agent definitions as the source of truth for normal function tools.
- Allow redelegation until max depth is reached.
- Use provider-native web search as a builtin capability.
- Start with OpenRouter only, using:

```json
"tools": [{"type": "openrouter:web_search"}]
```

- Save the final result as a markdown note so it can be referenced later.

## Architecture

### High-level flow

Both local chat and Telegram should feed the same backend pipeline:

1. Normalize inbound request into a shared run request.
2. Prepare or resume session and root agent.
3. Resolve function tools from the agent definition.
4. Resolve builtin native tools from runtime policy.
5. Run the orchestrator.
6. Persist note output and return the saved path.
7. Surface progress and completion through the transport adapter.

Transport adapters:

- Local app chat: streams events, subagent activity, and tool activity.
- Telegram: starts a run, sends concise status updates, and sends the final note path.

### Agent graph

- `planner`
- `researcher`
- `link_scout`
- `youtube_researcher`
- `web_researcher`
- `note_writer`

Expected delegation flow:

- `planner` decides a request is research.
- `planner` delegates to `researcher`.
- `researcher` redelegates to leaf workers while depth is below the max.
- Leaf workers gather findings and return results.
- `note_writer` persists the final markdown note and returns the path.

### Capability model

Normal function tools:

- Remain defined in `server/agents/*.md`.
- Example: `delegate`, `notes.save_research_note`, `web.fetch`.

Builtin native tools:

- Are runtime/provider-driven, not regular function tools.
- For now the only builtin native capability is `web_search`.
- OpenRouter maps builtin `web_search` to:

```json
[{"type": "openrouter:web_search"}]
```

### Delegation policy

No separate `can_delegate` flag is needed.

Rule:

- If an agent has `delegate` in its function toolset, it may delegate.
- If current depth is below `MAX_AGENT_DEPTH`, `delegate` remains visible.
- Once current depth reaches `MAX_AGENT_DEPTH`, `delegate` is removed or rejected.

This keeps delegation tool-centric and simple.

## Main decisions and justification

### 1. Shared pipeline for local chat and Telegram

Decision:

- Use one shared backend execution path for both local chat and Telegram.

Justification:

- The current system already shares important pieces such as session preparation and `runAgent`.
- Diverging orchestration logic by transport will drift quickly.
- Research behavior should be identical regardless of surface.

### 2. Keep agent definitions as the source of truth for normal tools

Decision:

- Do not move ordinary tool selection out of agent markdown definitions.

Justification:

- This matches the existing architecture.
- Agent roles remain explicit and reviewable.
- It avoids creating a second configuration system for tool visibility.

### 3. No `can_delegate` flag

Decision:

- Do not add a dedicated delegation capability flag.

Justification:

- It is redundant if `delegate` is already part of the agent's normal toolset.
- Depth-based removal is enough to bound recursion.
- The model becomes easier to reason about: tool presence plus depth limit controls behavior.

### 4. Provider-native web search as builtin capability

Decision:

- Model native web search as a builtin provider capability, not as a fake function tool.

Justification:

- Search is provider-native in the desired architecture.
- This keeps provider-specific syntax in the adapter layer instead of leaking it into prompts.
- It makes later support for Anthropic, OpenAI, or Gemini a mapping problem rather than an agent redesign.

### 5. OpenRouter only first

Decision:

- Support only OpenRouter native web search in the first version.

Justification:

- It reduces scope.
- The target declaration is already known.
- The architecture can be validated with one provider before broader expansion.

### 6. Dedicated note-saving path

Decision:

- Use a constrained research note-saving tool instead of generic raw file writing as the main output path.

Justification:

- Telegram is a poor surface for approval-heavy generic file operations.
- Research note persistence is a narrow, stable behavior and should be safe and predictable.
- A constrained tool can enforce allowed roots and return a stable final path.

### 7. Telegram as transport adapter, not special execution path

Decision:

- Make Telegram a thin transport layer over the same run path.

Justification:

- The current Telegram flow is more restrictive than the app path and will fight the research architecture.
- Telegram should control how messages are received and sent, not how agents fundamentally execute.

## Target runtime model

### Shared run request

Suggested shared request shape:

- `surface`: `app_chat | telegram`
- `session_id`
- `root_agent`
- `instructions`
- `transport_policy`

### Shared prepared agent config

Prepared config should include:

- model
- provider
- function tools from agent definition
- builtin native tools from runtime policy
- standard orchestrator limits

## Implementation plan

### 1. Create a shared run-preparation layer

Responsibilities:

- normalize inbound request
- resolve root agent
- prepare or resume session
- apply transport-specific instructions
- produce final prepared config for agent execution

Likely file area:

- `server/src/services/session-runner.ts`
- add a helper such as `server/src/services/run-profile.ts`

### 2. Extend provider request types for builtin native tools

Responsibilities:

- add a provider-agnostic builtin tool field
- keep normal function tools unchanged
- allow agent/runtime policy to request `web_search`

Likely file area:

- `server/src/providers/types.ts`

### 3. Add OpenRouter native web search mapping

Responsibilities:

- map builtin `web_search` to:

```json
[{"type": "openrouter:web_search"}]
```

- merge with the rest of the request shape
- document any OpenRouter-specific limitations if builtin search and function tools cannot always mix cleanly

Likely file area:

- `server/src/providers/openrouter.ts`

### 4. Replace blanket subagent delegation ban with depth-only gating

Responsibilities:

- stop removing `delegate` just because an agent is a subagent
- keep `delegate` visible if depth is below max
- remove or reject it once depth limit is reached

Likely file area:

- `server/src/orchestrator/runner.ts`

### 5. Add constrained note-saving tool

Suggested tool:

- `notes.save_research_note`

Responsibilities:

- sanitize file name
- accept markdown body
- resolve output path from:
  - explicit safe path
  - vault root
  - work root
  - fallback workspace
- write only under allowed roots
- return final absolute path

Likely file areas:

- `server/src/tools/notes.ts`
- `server/src/lib/runtime.ts`

### 6. Refine agent set

Suggested roles:

- `planner`
  - root coordinator
  - has `delegate`

- `researcher`
  - has `delegate`
  - has builtin native `web_search`

- `link_scout`
  - no `delegate`
  - has builtin native `web_search`

- `youtube_researcher`
  - no `delegate`
  - has builtin native `web_search`

- `web_researcher`
  - no `delegate`
  - has builtin native `web_search`

- `note_writer`
  - no builtin search
  - has `notes.save_research_note`

Likely file area:

- `server/agents/*.md`

### 7. Refactor Telegram onto the shared pipeline

Responsibilities:

- use the shared run-preparation path
- start the intended root agent
- stop acting as a highly restricted parallel execution path
- apply Telegram-specific instructions for concise messaging

Likely file area:

- `server/src/services/telegram.ts`

### 8. Make Telegram background-first for research runs

Responsibilities:

- acknowledge quickly
- run long research in background
- send concise final completion with note path
- preserve existing reply or fork semantics

Likely file area:

- `server/src/services/telegram.ts`

### 9. Keep local app chat on the same backend path

Responsibilities:

- continue using streaming UI
- continue showing subagent activity
- reuse the same backend orchestration and note-saving path

Likely file areas:

- `server/src/routes/chat.ts`
- `src/lib/stores/chat.ts`

## Phased delivery

### Phase 1

- shared run-preparation layer
- OpenRouter builtin web search support
- depth-only delegation gating
- updated agent definitions

### Phase 2

- constrained note-saving tool
- `note_writer`
- local app end-to-end research-to-note flow

### Phase 3

- Telegram refactor onto shared pipeline
- background Telegram runs
- Telegram status and completion messages

### Phase 4

- prompt tuning
- note format tuning
- citation and source formatting
- retry and failure handling

## Risks

- OpenRouter behavior may differ by model when combining builtin search with other tool usage.
- Recursive delegation can become expensive or noisy if prompts are not disciplined.
- Telegram background execution needs careful lifecycle and error handling.
- Native search results may be good for discovery but not always enough for deep page or transcript analysis.

## Success criteria

- Same research request works locally and via Telegram.
- `planner -> researcher -> leaf workers -> note_writer` runs through one backend path.
- Delegation is allowed until max depth and then stops cleanly.
- Research-capable agents can use OpenRouter native web search.
- Final output is a saved markdown note with a stable path.
- Telegram acts as a thin transport adapter rather than a separate execution model.
