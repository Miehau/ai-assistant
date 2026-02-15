# AI Frontend

Tauri desktop app — Rust backend + Svelte 5 frontend — for multi-provider AI agent orchestration with tool use.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Frontend | SvelteKit 2, Svelte 5 runes, shadcn-svelte (bits-ui), TailwindCSS 3, TypeScript strict |
| Backend | Tauri 1, Rust 2021 edition |
| Database | SQLite (rusqlite 0.32, bundled) with inline migrations |
| LLM providers | Anthropic, OpenAI, OpenAI-compatible, DeepSeek, Ollama, Claude CLI |
| Package manager | Bun |
| Build | Vite 6 (frontend, adapter-static SSG), Cargo (backend) |
| Bindings | tauri-specta (auto-generated TS types from Rust commands) |

## Commands

```bash
bun install                                        # Install frontend deps
bun run dev:web                                    # Frontend dev server (port 1420)
bun run dev:tauri                                  # Full app with Rust backend
bun run build:tauri                                # Production build
bun run preflight                                  # Web build + backend tests (CI-like check)
cargo test --manifest-path src-tauri/Cargo.toml    # Backend tests only
```

Full list: [`src-tauri/docs/agent/commands.md`](src-tauri/docs/agent/commands.md)

## Gotchas & Invariants

1. **Svelte 5 runes** — use `$state`, `$derived`, `$effect`. NOT React. NOT Svelte 4 stores.
2. **Anthropic strips `oneOf`/`anyOf`/`allOf`** from structured output schemas — never use them.
3. **Controller schema is flat** with optional fields — step type inferred from field presence (`tool` → tool call, `message` → respond, `question` → ask user).
4. **Always-succeeds fallback parsing → infinite loops** — fail hard on parse errors instead.
5. **Tool outputs >16 KB** are persisted to disk, not inlined in conversation.
6. **`AGENTS.md` must remain index-only** — test-enforced, no inline content.
7. **Provider changes** must follow `provider-contracts.md` PR checklist (provider-specific tests, preflight payload check, regression tests for 4xx errors).
8. **Trust `cargo check`** over rust-analyzer diagnostics (can be stale).
9. **Use provider-specific schema builders** (`build_openai_output_schema`, `build_anthropic_output_schema`) — never send shared schemas directly.

Full rules: [`src-tauri/docs/agent/invariants.md`](src-tauri/docs/agent/invariants.md)

## Architecture

```
┌─────────────┐     IPC      ┌──────────────┐    structured   ┌─────────────┐
│  Svelte 5   │ ──────────► │   Tauri       │    JSON         │  LLM        │
│  Frontend   │             │   Commands    │ ─────────────► │  Provider   │
│  (SvelteKit)│ ◄────────── │   (Rust)      │ ◄───────────── │  (API)      │
└─────────────┘   EventBus  └──────┬───────┘                 └─────────────┘
                                    │
                              ┌─────▼──────┐
                              │ Orchestrator│──► Tool Registry ──► Tool Handlers
                              │ (controller │──► Tool Output Store (JSON files)
                              │  loop)      │──► SQLite (conversations, state)
                              └────────────┘
```

### Backend core
- `src-tauri/src/main.rs` — Tauri setup, command registration
- `src-tauri/src/agent/orchestrator.rs` — controller loop, parsing, execution
- `src-tauri/src/agent/prompts.rs` — `CONTROLLER_PROMPT_BASE` system prompt
- `src-tauri/src/tools/` — tool implementations (`register_*` pattern)
- `src-tauri/src/tools/tool_outputs.rs` — output traversal tools (read, list, stats, extract, count, sample)
- `src-tauri/src/tool_outputs.rs` — storage layer (read/write JSON files)
- `src-tauri/src/llm/mod.rs` — provider-specific schema handling

### Frontend core
- `src/routes/` — SvelteKit pages (SSG, adapter-static)
- `src/lib/services/chat.ts` — chat service, IPC bridge
- `src/lib/components/` — UI components (shadcn-svelte based)

Full map: [`src-tauri/docs/agent/repo-map.md`](src-tauri/docs/agent/repo-map.md) | Flows: [`src-tauri/docs/agent/flows.md`](src-tauri/docs/agent/flows.md)

## Documentation Index

### Agent engineering (`src-tauri/docs/agent/`)
| Doc | Purpose |
|-----|---------|
| [`README.md`](src-tauri/docs/agent/README.md) | Index of all agent docs |
| [`invariants.md`](src-tauri/docs/agent/invariants.md) | Non-negotiable rules and test-enforced constraints |
| [`flows.md`](src-tauri/docs/agent/flows.md) | Golden paths: message send, tool output persistence, model selection |
| [`provider-contracts.md`](src-tauri/docs/agent/provider-contracts.md) | Provider-specific API change guardrails and PR checklist |
| [`repo-map.md`](src-tauri/docs/agent/repo-map.md) | Quick navigation map for core entrypoints |
| [`commands.md`](src-tauri/docs/agent/commands.md) | Development and testing commands |

### Skills (auto-activate via hooks)
| Skill | Triggers on |
|-------|------------|
| [`tauri-backend-guidelines`](.claude/skills/tauri-backend-guidelines/SKILL.md) | Rust backend work (commands, tools, database) |
| [`frontend-dev-guidelines`](.claude/skills/frontend-dev-guidelines/SKILL.md) | Svelte 5 components, routes, services |
| [`error-tracking`](.claude/skills/error-tracking/SKILL.md) | Sentry integration, error handling |
| [`route-tester`](.claude/skills/route-tester/SKILL.md) | Testing authenticated routes |

### Infrastructure
| File | Purpose |
|------|---------|
| [`AGENTS.md`](AGENTS.md) | Agent docs index (must stay index-only) |
| [`.claude/hooks/CONFIG.md`](.claude/hooks/CONFIG.md) | Hooks configuration guide |
| [`INTEGRATIONS.md`](INTEGRATIONS.md) | Plugin architecture spec (Gmail, Calendar, Todoist) |

## Key Patterns

**Adding a new Tauri command:**
1. Define handler in `src-tauri/src/` → 2. Register with `#[tauri::command]` → 3. Add to `.invoke_handler()` in `main.rs` → 4. Call via `invoke()` from frontend

**Adding a new agent tool:**
1. Implement `ToolHandler` trait → 2. Create `register_<name>()` in `src/tools/` → 3. Call register function in tool registry setup

**Frontend component conventions:**
- Props via `$props()`, state via `$state()`, derived via `$derived()`
- Use shadcn-svelte components, import from `$lib/components/ui/`
- For full patterns, read the linked skill files above

## Prerequisites

- Rust (stable toolchain)
- Bun (package manager)
- Node.js v18+
