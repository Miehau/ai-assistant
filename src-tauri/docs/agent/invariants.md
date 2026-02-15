# Invariants

These are not optional. If a change breaks one, it must be fixed or justified.

- /Users/michalmlak/Projects/ai-frontend/AGENTS.md must remain index-only (test enforced).
- Provider contract changes must follow /Users/michalmlak/Projects/ai-frontend/src-tauri/docs/agent/provider-contracts.md.
- Controller outputs must be valid JSON that matches the controller schema (see /Users/michalmlak/Projects/ai-frontend/src-tauri/src/agent/orchestrator.rs).
- Tool args must satisfy each tool's JSON schema (validated in /Users/michalmlak/Projects/ai-frontend/src-tauri/src/tools/mod.rs).
- Update /Users/michalmlak/Projects/ai-frontend/src-tauri/docs/agent/README.md when adding or renaming agent docs.
