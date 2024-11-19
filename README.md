# AI Chat Interface

A desktop chat application built with Tauri, SvelteKit, and TypeScript that provides a secure, local-first approach to AI interactions.

## Features

- 🔒 Privacy-focused: All data stored locally
- 🤖 Multiple AI model support
- 💾 Persistent chat history (WIP)
- 🔄 Stream responses in real-time
- 🎯 Custom system prompts
- 🎨 Clean, modern UI with dark/light mode (WIP)

## Setup & Installation

### Prerequisites
- Node.js (v16 or higher)
- Rust (latest stable)
- Bun (for package management)

### Installation Steps
0. Make sure you have Rust installed
1. Clone the repository
2. Install dependencies with: bun install
3. Run development server: bun run tauri dev
4. Build for production: bun run build

## Configuration

### AI Model Setup
1. Navigate to Settings
2. Add your API keys for desired providers
3. Enable/disable specific models
4. Configure custom endpoints if needed

### System Prompts
Create and manage system prompts through the Assistants interface for:
- Creating new prompts
- Editing existing prompts
- Deleting unused prompts

## Privacy & Data Storage

This application prioritizes user privacy:
- All chat history stored locally in SQLite
- No data sent to external servers except AI API calls
- API keys stored securely in local database
- No telemetry or usage tracking
- No cloud sync (fully offline capable)

## Development

### Project Structure
```
src/
├── lib/
│   ├── components/    # Reusable UI components
│   ├── services/      # API and business logic
│   ├── types/         # TypeScript definitions
│   └── utils/         # Helper functions
├── routes/            # SvelteKit pages
└── app.html           # Base HTML template
```

### Key Technologies
- Frontend: SvelteKit, TypeScript, TailwindCSS
- Backend: Tauri (Rust)
- Database: SQLite (via rusqlite)
- UI Components: shadcn-svelte
- Icons: Lucide

## Contributing

1. Fork the repository
2. Create your feature branch
3. Commit your changes
4. Push to the branch
5. Open a Pull Request

