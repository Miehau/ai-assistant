# Commands

Prefer the smallest command that validates the change. All commands run from repo root.

Common
- `bun run dev:web` (frontend dev server)
- `bun run dev:tauri` (full app with Rust backend)
- `bun run build:web` (frontend build)
- `bun run build:tauri` (full build)
- `bun run preflight` (web build + backend tests)

Backend tests only
- `cargo test --manifest-path src-tauri/Cargo.toml`

Notes
- First-time setup: `bun install`
- `bun run preflight` is the closest CI-like check.
