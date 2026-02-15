# Invariants

These are not optional. If a change breaks one, it must be fixed or justified.

- ***REMOVED***Projects/ai-frontend/AGENTS.md must remain index-only (test enforced).
- Provider contract changes must follow ***REMOVED***Projects/ai-frontend/src-tauri/docs/agent/provider-contracts.md.
- Controller outputs must be valid JSON that matches the controller schema (see ***REMOVED***Projects/ai-frontend/src-tauri/src/agent/orchestrator.rs).
- Tool args must satisfy each tool's JSON schema (validated in ***REMOVED***Projects/ai-frontend/src-tauri/src/tools/mod.rs).
- Update ***REMOVED***Projects/ai-frontend/src-tauri/docs/agent/README.md when adding or renaming agent docs.
