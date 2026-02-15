# harness-dev Plugin

A Claude Code plugin that extends the feature development workflow with invariant enforcement, entropy detection, progressive disclosure, and knowledge gardening.

## Usage

```
/harness-dev <feature request or task description>
```

## The 10-Phase Workflow

| # | Phase | Agents Used | Description |
|---|-------|-------------|-------------|
| 1 | Discovery | — | Understand the user's intent |
| 2 | Codebase Exploration | code-explorer (2-3x) | Progressive disclosure from docs to code |
| 3 | Clarifying Questions | — | Surface invariant conflicts, resolve ambiguity |
| 4 | Architecture Design | code-architect (2-3x) | Blueprint with invariant compliance |
| 5 | **Invariant Check** | **invariant-enforcer** | Gate: APPROVED or BLOCKED verdict |
| 6 | Implementation | — | Code according to approved blueprint |
| 7 | Quality Review | code-reviewer (3x) | Bugs, security, conventions |
| 8 | **Harness Review** | **entropy-scanner + knowledge-gardener** | Drift detection + doc updates |
| 9 | **Post-Impl Verification** | **invariant-enforcer** | Re-check against actual code |
| 10 | Summary | — | Final report with harness health grades |

Phases 5, 8, and 9 are unique to harness-dev (not in standard feature-dev).

## Agents

### Inherited (enhanced)

- **code-explorer** — Codebase exploration with progressive disclosure. Starts from `repo-map.md` and `flows.md` before diving into code.
- **code-architect** — Architecture design with invariant awareness. Must read `invariants.md` and `provider-contracts.md` before designing.
- **code-reviewer** — Code review with confidence-based filtering. Reports only HIGH/CRITICAL issues.

### New

- **invariant-enforcer** — Read-only. Validates designs and implementations against `invariants.md` and `provider-contracts.md`. Produces PASS/FAIL checklist and APPROVED/BLOCKED verdict. Runs twice: Phase 5 (design) and Phase 9 (implementation).
- **entropy-scanner** — Read-only. Scans for doc drift, pattern duplication, complexity growth, AI slop, dependency violations, and test gaps. Scores 6 domains A-F.
- **knowledge-gardener** — Has Write/Edit access. Updates `repo-map.md`, `flows.md`, `invariants.md`, `provider-contracts.md`, and `README.md`. Verifies cross-links. Minimal surgical edits only.

## Referenced Documentation

The plugin reads and enforces rules from these project docs:

- `src-tauri/docs/agent/invariants.md` — Non-negotiable project rules
- `src-tauri/docs/agent/provider-contracts.md` — Provider API contract guardrails
- `src-tauri/docs/agent/flows.md` — Golden execution flows
- `src-tauri/docs/agent/repo-map.md` — Codebase navigation map
- `AGENTS.md` — Must remain index-only (enforced by invariant-enforcer)

## Entropy Domains

The entropy-scanner grades these 6 domains:

| Domain | What It Measures |
|--------|-----------------|
| Docs Freshness | Are docs accurate? Do paths point to real files? |
| Pattern Consistency | Do all modules follow the same patterns? |
| Complexity Budget | Are files/functions within size limits? |
| Dependency Hygiene | Do dependencies flow in the right direction? |
| Test Health | Is test coverage proportional to complexity? |
| Slop Index | Dead code, verbose comments, copy-paste, unwrap() abuse |

Grades: A (exemplary) through F (critical entropy).
