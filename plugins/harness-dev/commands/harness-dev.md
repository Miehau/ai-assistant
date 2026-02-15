# /harness-dev — Harness Engineering Workflow

You are orchestrating a 10-phase feature development workflow that combines rigorous software engineering with invariant enforcement, entropy detection, and knowledge gardening. Follow every phase in order. Do not skip phases.

## Input

The user provides a feature request or task description as the argument to `/harness-dev`.

---

## Phase 1: Discovery

**Goal:** Understand the user's intent before touching any code.

1. Restate the request in your own words.
2. Identify the scope: new feature, bug fix, refactor, or docs change.
3. List any assumptions you are making.
4. If the request is ambiguous, ask clarifying questions now (max 3).

**Output:** A numbered summary of your understanding.

---

## Phase 2: Codebase Exploration

**Goal:** Build a mental model of the relevant code using progressive disclosure.

1. **Start from docs** — read `src-tauri/docs/agent/repo-map.md` and `src-tauri/docs/agent/flows.md` first. These give you the shortest path to the right files.
2. Launch **2–3 code-explorer agents** in parallel (use `subagent_type: "harness-dev:code-explorer"`). Each explorer should investigate a different aspect of the feature surface (e.g., frontend entry point, backend handler, data model).
3. Synthesize the explorers' findings into a single **Exploration Summary** listing:
   - Key files and their roles
   - Existing patterns the feature must follow
   - Integration points (events, commands, stores, DB)
   - Any surprises or red flags

**Output:** Exploration Summary (bulleted, with file paths).

---

## Phase 3: Clarifying Questions

**Goal:** Resolve ambiguity before designing.

1. Based on the exploration, surface any **potential invariant conflicts** — things that might violate rules in `src-tauri/docs/agent/invariants.md` or `src-tauri/docs/agent/provider-contracts.md`.
2. Ask the user any remaining questions (max 5). Group them:
   - **Functional:** What should happen?
   - **Architectural:** Where should it live?
   - **Invariant:** Does this conflict with existing rules?
3. If there are no questions, state that explicitly and proceed.

**Output:** Numbered questions or explicit "No questions — proceeding."

---

## Phase 4: Architecture Design

**Goal:** Produce a concrete implementation blueprint that respects project invariants.

1. Launch **2–3 code-architect agents** (use `subagent_type: "harness-dev:code-architect"`). Each architect should explore a different design approach or focus on a different layer (frontend, backend, data).
2. Architects **must read** `src-tauri/docs/agent/invariants.md` and `src-tauri/docs/agent/provider-contracts.md` before designing.
3. Synthesize into a single **Architecture Blueprint** containing:
   - Files to create (with purpose)
   - Files to modify (with summary of changes)
   - Data flow diagram (text-based)
   - How the design respects each relevant invariant
   - Build/test sequence

**Output:** Architecture Blueprint.

---

## Phase 5: Invariant Check (GATE)

**Goal:** Verify the design does not violate any project invariants before writing code.

1. Launch the **invariant-enforcer agent** (use `subagent_type: "harness-dev:invariant-enforcer"`).
2. Pass it the Architecture Blueprint from Phase 4.
3. The enforcer will return a structured report with PASS/FAIL per check and an overall verdict: **APPROVED** or **BLOCKED**.

**If BLOCKED:**
- Display the enforcer's report to the user.
- Explain which invariants are violated and how.
- Return to Phase 4 to redesign. Do not proceed to implementation.

**If APPROVED:**
- Display the report and proceed to Phase 6.

**Output:** Invariant Check Report + verdict.

---

## Phase 6: Implementation

**Goal:** Write the code according to the approved blueprint.

1. Only start if Phase 5 returned APPROVED.
2. Follow the Architecture Blueprint exactly. If you need to deviate, explain why.
3. Implement in the order specified by the build sequence.
4. After writing code, run the relevant verification commands from `src-tauri/docs/agent/commands.md`:
   - `cargo test --manifest-path src-tauri/Cargo.toml` for backend changes
   - `bun run build:web` for frontend changes
   - `bun run preflight` for full verification

**Output:** List of files created/modified + test results.

---

## Phase 7: Quality Review

**Goal:** Catch bugs, logic errors, security issues, and convention violations.

1. Launch **3 code-reviewer agents** (use `subagent_type: "harness-dev:code-reviewer"`). Each reviewer should focus on a different concern:
   - Reviewer 1: Correctness and logic
   - Reviewer 2: Security and error handling
   - Reviewer 3: Project conventions and code quality
2. Synthesize findings. Only report issues with HIGH or CRITICAL confidence.
3. Fix any issues found before proceeding.

**Output:** Review findings + fixes applied.

---

## Phase 8: Harness Review

**Goal:** Detect entropy drift and update project knowledge.

1. Launch the **entropy-scanner agent** (use `subagent_type: "harness-dev:entropy-scanner"`).
   - It will score 6 domains: Docs Freshness, Pattern Consistency, Complexity Budget, Dependency Hygiene, Test Health, Slop Index.
   - Each domain gets an A–F grade.
2. Launch the **knowledge-gardener agent** (use `subagent_type: "harness-dev:knowledge-gardener"`).
   - It will update `repo-map.md`, `flows.md`, `invariants.md`, `provider-contracts.md`, and `README.md` as needed.
   - It will verify all cross-links point to real files.
3. Display the entropy scores and any doc updates made.

**Output:** Entropy Report (6 domain scores) + list of doc updates.

---

## Phase 9: Post-Implementation Invariant Verification

**Goal:** Re-check invariants against the actual code (not just the design).

1. Launch the **invariant-enforcer agent** again (use `subagent_type: "harness-dev:invariant-enforcer"`).
2. This time it validates against the actual implementation, not the blueprint.
3. If any invariant is now violated (e.g., AGENTS.md was modified, schema uses oneOf), flag it.

**If BLOCKED:**
- Fix the violations before proceeding.
- Re-run the enforcer until APPROVED.

**If APPROVED:**
- Proceed to Phase 10.

**Output:** Post-Implementation Invariant Report + verdict.

---

## Phase 10: Summary

**Goal:** Provide a complete summary of everything done.

Produce a final report containing:

1. **What was built:** One-paragraph summary.
2. **Files changed:** Table with file path, action (created/modified/deleted), and one-line description.
3. **Invariants:** Final enforcer verdict (Phase 9).
4. **Harness Health Report:** The 6 entropy domain scores from Phase 8:
   | Domain | Grade | Notes |
   |--------|-------|-------|
   | Docs Freshness | | |
   | Pattern Consistency | | |
   | Complexity Budget | | |
   | Dependency Hygiene | | |
   | Test Health | | |
   | Slop Index | | |
5. **Knowledge updates:** Docs that were updated by the knowledge-gardener.
6. **Remaining work:** Anything the user should follow up on.

**Output:** Final Summary Report.
