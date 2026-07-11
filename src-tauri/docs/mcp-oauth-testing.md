# MCP OAuth test and release guide

The MCP OAuth suite is deterministic and self-contained. `OAuthMcpFixture` starts loopback-only authorization and resource servers with discovery metadata, dynamic registration, PKCE authorization codes, short-lived access tokens, rotating refresh tokens, consent outcomes, and OAuth/no-auth/bearer MCP endpoints. The stdio fixture runs as a local child process. No external credential or network service is used.

## Run locally

Use Node 22, which is also pinned by the CI job:

```sh
cd server
bun install --frozen-lockfile
bun run test:mcp-oauth
cd ..
bun run test:mcp-frontend
```

If a native `better-sqlite3` ABI error appears after changing Node versions, reinstall `server/node_modules` with the selected Node 22 runtime. A fixture bind failure means the environment disallows loopback listeners; allow local ephemeral ports and rerun. Tests print only safe lifecycle diagnostics (server ID and error class), never authorization URLs or provider values.

## Coverage matrix

| Requirement/control | Automated evidence |
|---|---|
| OAuth discovery, DCR, PKCE, code exchange, harmless tool call | `server/src/test/mcp-oauth-integration.ts` |
| No-auth remote discovery and tool call | `mcp-oauth-integration.ts` |
| Static bearer and stdio connection/tool regressions | `mcp-oauth-integration.ts` |
| Backend restart without sign-in; token expiry and refresh rotation | `mcp-oauth-integration.ts` |
| Disconnect removes tool availability; denial/invalid-code retry | `mcp-oauth-integration.ts`, `mcp-routes.ts` |
| Callback success, denial, malformed input, replay, cancellation, expiry | `server/src/test/mcp-routes.ts` |
| Callback CSP/no-store/referrer/nosniff headers | `mcp-routes.ts` |
| Callback and authenticated API rate limits | `mcp-routes.ts` |
| Cross-user list/select/authorize/mutate/invoke isolation | `mcp-routes.ts`, `mcp-oauth-integration.ts` |
| Cross-server/user credential and resource binding | `server/src/test/mcp-oauth-provider.ts`, `mcp-repository.ts` |
| SQLite encryption, migration, cascade, cleanup; Postgres schema parity | `server/src/test/mcp-repository.ts` |
| Typed safe frontend payloads, terminal/timeout/cancel polling | `src/lib/services/mcpOAuthPolling.test.ts` |
| Wizard stages/status/actions, a11y hooks, popup fallback, drawer boundary | `src/lib/components/mcpManagement.test.ts` |
| Secret-shaped fields absent from APIs/frontend types | `mcp-routes.ts`, `mcpOAuthPolling.test.ts` |
| Secret canaries absent from normal/narrow visual artifacts | `mcpManagement.test.ts` |
| Normal and narrow visual layout | `src/lib/components/__snapshots__/mcp-settings-*.jpg` |

The standard CI matrix uses SQLite because it exercises the complete runtime locally. `mcp-repository.ts` also executes the Postgres startup DDL against a deterministic fake connection to verify ownership, auth-mode, credential/session, constraint, and index parity without requiring a CI database service.

## Debug safely

Use server IDs, lifecycle statuses, HTTP status codes, and error classes. Never paste authorization URLs, callback query strings, database credential blobs, or provider response bodies into logs or tickets. An `UnauthorizedError` before authorization is an expected challenge. A resource mismatch is a fixture/metadata error; do not bypass validation. Replay and expired-state cases must remain `410` with generic HTML.

## Release checklist

1. Run `bun run test:mcp-oauth` in `server/` and `bun run test:mcp-frontend` plus `bun run build:web` at the repository root on Node 22.
2. Confirm migrations succeed on a backed-up copy and `ENCRYPTION_KEY` remains stable.
3. Verify `PUBLIC_BASE_URL` produces the registered `/oauth/mcp/callback` URI and production uses HTTPS.
4. Verify callback routing preserves CSP, no-store, referrer, nosniff, and dedicated rate limiting without application bearer middleware.
5. Exercise Settings → Connections → MCP servers at normal and narrow window sizes; confirm chat drawer selection locks after the first message.
6. Confirm OAuth, no-auth, bearer, and stdio fixtures are green; restart/refresh rotation, replay/expiry, disconnect, and second-user isolation must be automated passes.
7. Search build/test artifacts and logs for token, code, state, verifier, and client-secret canaries before release.
8. Back up the database and encryption key; do not roll back to a global/non-user-scoped MCP binary after migration.
