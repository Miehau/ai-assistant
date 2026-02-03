# AI Agent UI

A privacy-focused desktop AI chat application built with Tauri, SvelteKit, and Rust. Features an autonomous agent system with tool use capabilities.

## Features

- **Privacy-first**: All data stored locally in SQLite, no telemetry
- **Multiple AI providers**: OpenAI, Anthropic Claude, Claude CLI, and OpenAI-compatible endpoints
- **Autonomous agent**: Multi-step reasoning with tool execution
- **Tool system**: File operations, web search, Obsidian vault integration
- **Streaming responses**: Real-time token streaming from all providers
- **Usage tracking**: Monitor token usage and costs across models
- **Custom system prompts**: Create and manage assistants with different personalities
- **Conversation branching**: Compare different conversation paths side-by-side

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (for package management)
- Node.js v18+

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd ai-frontend

# Install dependencies
bun install

# Run in development mode
bun run dev:tauri

# Build for production
bun run build
```

## Configuration

### API Keys

1. Open the app and navigate to **Settings**
2. Add API keys for your preferred providers:
   - OpenAI API key
   - Anthropic API key
3. Optionally configure custom OpenAI-compatible endpoints

### Gmail OAuth Tokens (Production)

For production distribution, OAuth tokens should be stored in the OS keychain
instead of the SQLite database. This prevents access/refresh tokens from being
read if the database is copied. This is a recommended follow-up before shipping
public builds.

### Claude CLI (Optional)

If you have [Claude Code](https://github.com/anthropics/claude-code) installed, the app can use it as a provider:

1. Ensure `claude` CLI is available in your PATH
2. Models prefixed with `claude-cli-` will route through the CLI

### Obsidian Vault

To enable vault tools for note search and file operations:

1. Go to **Settings > Vault**
2. Select your Obsidian vault directory

## Project Structure

```
src/                    # SvelteKit frontend
├── lib/
│   ├── components/     # UI components (shadcn-svelte)
│   ├── services/       # Frontend services
│   ├── stores/         # Svelte stores
│   └── types/          # TypeScript definitions
└── routes/             # App pages

src-tauri/              # Rust backend
├── src/
│   ├── agent/          # Autonomous agent system
│   ├── commands/       # Tauri IPC commands
│   ├── db/             # SQLite database layer
│   ├── llm/            # LLM provider implementations
│   └── tools/          # Agent tools (files, search, vault)
```

## Tech Stack

- **Frontend**: SvelteKit 5, TypeScript, TailwindCSS, shadcn-svelte
- **Backend**: Tauri, Rust
- **Database**: SQLite (rusqlite)
- **UI**: Lucide icons, bits-ui, layerchart

## Development

```bash
# Frontend only (web mode)
bun run dev:web

# Full Tauri app
bun run dev:tauri
```

## License

MIT
