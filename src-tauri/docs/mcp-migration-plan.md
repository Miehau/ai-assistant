# MCP Migration Plan: files-stdio-mcp-server Integration

**Goal:** Replace native Rust file tools with [files-stdio-mcp-server](https://github.com/iceener/files-stdio-mcp-server) bundled as a Tauri sidecar.

**Benefits:**
- Better sandboxing and security
- Community-maintained implementation
- Portable across AI applications
- Reduced maintenance burden
- Standard MCP protocol

---

## Progress Tracker

### Phase 1: Bundle MCP Server (Day 1)
- [ ] Clone/install files-stdio-mcp-server in `src-tauri/binaries/`
- [ ] Configure `tauri.conf.json` sidecar bundling
- [ ] Verify sidecar binary is included in builds
- [ ] Test sidecar spawning with Tauri API

### Phase 2: MCP Client (Day 1-2)
- [ ] Search for existing Rust MCP client libraries
- [ ] Implement or integrate MCP client (JSON-RPC over stdio)
- [ ] Test initialize, list_tools, call_tool methods
- [ ] Add basic error handling

### Phase 3: Server Lifecycle (Day 2)
- [ ] Create `McpServerManager` to spawn/manage processes
- [ ] Implement graceful shutdown on app exit
- [ ] Add health checks and auto-restart (optional)
- [ ] Handle MCP server crashes gracefully

### Phase 4: ToolRegistry Integration (Day 2-3)
- [ ] Create `McpToolAdapter` to bridge MCP → ToolRegistry
- [ ] Convert MCP schemas to `ToolMetadata`
- [ ] Wrap MCP calls in `ToolHandler` closures
- [ ] Handle async→sync bridging

### Phase 5: Path Translation (Day 3)
- [ ] Implement vault/work root → absolute path resolver
- [ ] Apply path translation before MCP calls
- [ ] Test with edge cases

### Phase 6: Replace File Tools (Day 3-4)
- [ ] Register MCP tools in `main.rs` on startup
- [ ] Remove `register_file_tools()` call
- [ ] Delete `src-tauri/src/tools/files/` directory
- [ ] Update `tools/mod.rs` to remove files module

### Phase 7: Testing (Day 4)
- [ ] Update tests to use MCP tools
- [ ] Test all file operations (read, write, edit, list)
- [ ] Test large file handling (>16KB persistence)
- [ ] Test permission errors
- [ ] Run full test suite

### Phase 8: Cross-Platform Validation (Day 5)
- [ ] Test macOS build
- [ ] Test Windows build
- [ ] Test Linux build
- [ ] Verify MCP server cleanup on shutdown

---

## Architecture

**Current:**
```
Orchestrator → ToolRegistry → Native Rust File Tools → File System
```

**Target:**
```
Orchestrator → ToolRegistry → McpToolAdapter → McpClient → MCP Server (sidecar) → File System
```

---

## Key Components

### 1. MCP Client (`src-tauri/src/mcp/client.rs`)
- JSON-RPC 2.0 over stdio
- Methods: `initialize()`, `list_tools()`, `call_tool()`

### 2. MCP Server Manager (`src-tauri/src/mcp/manager.rs`)
- Spawn MCP server as Tauri sidecar
- Track running processes
- Handle shutdown

### 3. Tool Adapter (`src-tauri/src/mcp/adapter.rs`)
- Convert MCP tools → ToolDefinition
- Bridge MCP calls to ToolRegistry interface

### 4. Path Resolver (`src-tauri/src/mcp/path_resolver.rs`)
- Translate `root: "vault"` → absolute paths
- Apply before calling MCP tools

---

## Module Structure

```
src-tauri/
├── binaries/
│   └── files-mcp-server/          # Bundled MCP server
├── src/
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── client.rs              # JSON-RPC client
│   │   ├── manager.rs             # Process lifecycle
│   │   ├── adapter.rs             # ToolRegistry bridge
│   │   └── path_resolver.rs      # Path translation
│   ├── tools/
│   │   ├── files/                 # ❌ DELETE
│   │   └── ...                    # Keep other native tools
│   └── main.rs                    # MCP initialization
└── tauri.conf.json                # Sidecar config
```

---

## Key Decisions

- [ ] Use existing Rust MCP library or build minimal client?
- [ ] Bundle Node.js runtime or create standalone binary?
- [ ] Keep native file tools as fallback during migration?
- [ ] Async refactor for ToolHandler or use `block_on()`?

---

## Estimated Effort

| Phase | Time | Risk |
|-------|------|------|
| Bundle sidecar | 1 day | Low |
| MCP client | 1-2 days | Medium |
| Lifecycle manager | 0.5 day | Low |
| ToolRegistry adapter | 1 day | Low |
| Path translation | 0.5 day | Low |
| Integration | 1 day | Medium |
| Testing | 1-2 days | Medium |
| **Total** | **5-8 days** | **Medium** |

---

## Next Steps

1. Set up sidecar bundling
2. Build/integrate MCP client
3. Create proof of concept with one tool
4. Complete full integration

---

**Status:** Planning Phase
**Last Updated:** 2026-03-12
