# Obsidian Vault Tooling Progress

## Status
- [x] Tool registry + agent tool loop with approval events
- [x] File plugin (vault-aware read/write/edit/append/create) + settings
- [ ] Search tool using `rg` with default excludes
- [ ] Approval flow + diff UI for edits

## Notes
- Vault root stored in `user_preferences` under `plugins.files.vault_root`.
- "Open" means read contents.
- Edits require approval; reads/appends do not.
