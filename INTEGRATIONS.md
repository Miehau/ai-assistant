# Integrations and Plugin Architecture

This document defines the plugin-first architecture for external integrations, including Gmail, Google Calendar, Todoist, and MCP servers. It is intended as a practical spec for implementation and iteration.

## Goals
- Build integrations as plugins with a shared runtime, UI, and security model.
- Support both OAuth-based services and API-key-based services.
- Enable in-app configuration of MCP servers (local and remote).
- Keep integration scope minimal, auditable, and extensible.

## Non-Goals (v0)
- Slack and Linear support.
- Enterprise SSO or SCIM.
- Multi-tenant admin tooling.

## Plugin Model
A plugin exposes a manifest and a set of capabilities. The runtime uses the manifest for UI and policy decisions, and the capabilities for execution.

### Manifest (conceptual)
- id: unique stable identifier, e.g. `gmail`.
- name: display name.
- provider: vendor or ecosystem, e.g. `google`.
- auth: `oauth2` or `api_key`.
- scopes: requested scopes in human-readable labels and provider scope strings.
- capabilities: list of supported capabilities.
- webhook: optional details about webhook support and verification.
- settings: plugin-specific settings schema.

### Capabilities
- sync: pull or push data on a schedule.
- webhook_ingest: receive and process external events.
- action_execute: perform user-initiated actions (send email, create event, etc.).
- discovery: list available resources (labels, calendars, projects).
- health_check: validate connectivity and auth state.

### Lifecycle
- install: register plugin and render configuration UI.
- connect: authenticate and store credentials.
- configure: set plugin settings (calendars, labels, projects).
- run: execute capabilities on schedule or by user request.
- disconnect: revoke tokens and remove credentials.

## Runtime Architecture
- Plugin registry: resolves available plugins and their manifests.
- Credential store: encrypted storage for tokens and API keys.
- Scheduler: executes `sync` and `health_check` jobs.
- Webhook router: receives and dispatches webhook events.
- Action router: executes `action_execute` for user requests.
- Audit log: records all external side effects.

## UI
- Integrations hub: list plugins, status, last sync, and permissions.
- Plugin detail: connect/disconnect, configure settings, view status.
- MCP configuration: add/edit/delete server, auth type, test connection.

## Security and Compliance
- Scope minimization per plugin.
- Token encryption at rest.
- Audit trail for actions and external writes.
- Allowlist and TLS enforcement for remote MCP servers.

## Phase 1 (Foundation)
- Implement plugin registry, manifest format, and capability routing.
- OAuth2 and API key credential flows.
- Integrations hub UI.
- MCP server configuration UI and health check.

## Phase 2 (Core Plugins)
- Gmail plugin: read threads, send email, label threads.
- Google Calendar plugin: read events, create events.
- Todoist plugin: create tasks, complete tasks, list projects.

## Open Questions
- Do we bundle plugins in-app or load from a plugin directory at runtime?
- Where should plugin settings schemas live and how are they validated?
- How should webhook verification secrets be stored and rotated?

