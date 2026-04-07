# Invariants

These are not optional. If a change breaks one, it must be fixed or justified.

- AGENTS.md must remain index-only (test enforced).
- Provider contract changes must follow src-tauri/docs/agent/provider-contracts.md.
- Controller outputs must be valid JSON that matches the controller schema (see src-tauri/src/agent/orchestrator.rs).
- Tool args must satisfy each tool's JSON schema (validated in src-tauri/src/tools/mod.rs).
- Update src-tauri/docs/agent/README.md when adding or renaming agent docs.
