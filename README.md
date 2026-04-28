# AI Agent UI

Desktop AI chat app with autonomous agent and tool use. Tauri shell + SvelteKit frontend + Hono backend.

## Provider Support

| Provider | Status |
|----------|--------|
| **OpenRouter** | Fully supported |
| Anthropic, OpenAI, DeepSeek, Ollama, Claude CLI | Experimental / maybe |

### Adding a Model

1. Open the **sidebar**
2. Click **Models**
3. Type the model name (e.g. `anthropic/claude-sonnet-4`)

Requires an OpenRouter API key in **Settings**.

## Getting Started

```bash
# Install dependencies
bun install
cd server && bun install && cp .env.example .env
# Edit server/.env — add your OpenRouter API key
cd ..

# Run everything (server + Tauri desktop app)
bun run dev
```

This starts the Hono API (port 3001) and Tauri window in parallel. If either crashes, both stop.

### Run parts individually

```bash
bun run dev:web      # Frontend only in browser (http://localhost:1420)
cd server && bun run dev   # Server only
bun run dev:tauri    # Tauri desktop app only (no server)
```

### Production Backend

For a networked deployment, run the server with Postgres and create a bearer token before exposing API routes:

```bash
cd server
DB_DIALECT=postgres DATABASE_URL=postgres://user:pass@host:5432/dbname ENCRYPTION_KEY=<32-byte-secret> bun run create-key
```

In the frontend, open Settings -> Backend and save the server URL plus the generated bearer token.
Set `TRUST_PROXY=true` only when a trusted reverse proxy or platform is setting `X-Forwarded-For`/`X-Real-IP`.

## Prerequisites

- [Bun](https://bun.sh/)
- [Rust](https://rustup.rs/) (stable) — for Tauri shell
- Node.js v18+

## Project Structure

```
src/                    # SvelteKit frontend (Svelte 5 runes)
├── lib/
│   ├── components/     # UI components (shadcn-svelte)
│   ├── services/       # Frontend services
│   ├── stores/         # Svelte stores
│   └── types/          # TypeScript definitions
└── routes/             # App pages

server/                 # Hono backend (the real backend)
└── src/
    ├── agents/         # Agent definitions
    ├── orchestrator/   # Controller loop
    ├── providers/      # LLM provider implementations
    ├── tools/          # Agent tools
    ├── db/             # Drizzle ORM schemas for SQLite and Postgres
    ├── routes/         # API routes
    └── workflows/      # Workflow definitions

src-tauri/              # Tauri shell (thin native wrapper)
```

## Tech Stack

- **Frontend**: SvelteKit 2, Svelte 5, TypeScript, TailwindCSS, shadcn-svelte
- **Backend**: Hono, Drizzle ORM, better-sqlite3/Postgres, TypeScript
- **Shell**: Tauri 1 (native desktop wrapper)
- **UI**: Lucide icons, bits-ui

## License

MIT
