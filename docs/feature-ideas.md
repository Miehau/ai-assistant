# Feature Ideas — AI Frontend

Generated 2026-02-17 from codebase analysis.

## Top 10

### 1. Conversation Search & Export
- Full-text search across all messages in SQLite
- Filter by date, model, tool usage, cost
- Export to Markdown/JSON/PDF
- **Gap**: Rich conversation data stored but no retrieval mechanism

### 2. Custom Tool Builder (No-Code)
- Define tools via JSON schema in the UI
- Map to HTTP endpoints, shell commands, or scripts
- Hot-reload without recompilation
- **Gap**: Adding tools currently requires Rust code + rebuild

### 3. Agent Memory / Knowledge Base
- `memories` table exists in DB but is unused
- Auto-extract key facts from conversations
- RAG-style retrieval for context injection
- User-curated knowledge entries
- **Gap**: Every conversation starts from zero

### 4. Prompt Template Library & Chaining
- Parameterized prompt templates with variables
- Prompt chains (multi-step workflows)
- Importable prompt packs
- Version history per prompt
- **Gap**: Only plain-text system prompts exist today

### 5. Conversation Forking & A/B Testing
- Fork a conversation to compare models side-by-side
- Compare outputs (quality, cost, speed) across providers
- Auto-evaluate with rubrics
- **Gap**: Branching infra exists but no model comparison UX

### 6. Scheduled / Recurring Agent Tasks
- Cron-like scheduled agent runs
- Event-triggered workflows (e.g., new calendar event -> Todoist task)
- Background sessions with notifications
- **Gap**: Integrations (Gmail, Calendar, Todoist) are reactive only

### 7. Workspace/Project Context Manager
- Project profiles (root dirs, relevant files, conventions)
- Auto-inject project context into agent prompts
- `.aicontext` file in project roots for auto-detection
- **Gap**: Vault + work roots exist but no project-level switching

### 8. Tool Pipeline / Workflow Builder
- Visual drag-and-drop tool chains
- Save and reuse pipelines
- Conditional branching based on tool outputs
- **Gap**: `tool_batch` supports parallel execution but no saved pipelines

### 9. Usage Dashboard & Budget Alerts
- Daily/weekly/monthly budget limits per provider
- Alerts when approaching thresholds
- Cost optimization suggestions
- Model cost-efficiency rankings from actual usage data
- **Gap**: Usage charts exist but no budgets or intelligence

### 10. Conversation Templates & Quick Actions
- Templated starting points for common workflows
- Customizable quick-action buttons on main chat screen
- Pre-loaded context per template
- **Gap**: Every conversation starts blank

## Honorable Mentions
- MCP Server Auto-Discovery — scan environment for available servers
- Conversation Sharing — export as shareable link/file
- Voice Input/Output — Whisper integration
- Plugin Marketplace — community-contributed tool definitions
