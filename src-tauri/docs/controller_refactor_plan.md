# Controller Refactor — Implementation Plan

## Problem

The agent enters infinite loops when Anthropic strips `oneOf` from the controller schema, causing `step` to become optional. Missing `step` triggers fallback parsing that synthesizes `Think` steps, which call `call_think` — a context-free LLM call that produces nonsense. That nonsense poisons `LAST TOOL OUTPUT` and the controller retries the same tool indefinitely.

## Design Principles

1. **No `oneOf`/`allOf`/`anyOf` in schema** — flat optional fields, validate in Rust post-parse.
2. **No `Think` step type** — thinking happens via the `thinking` field on the same controller response.
3. **Flat schema** — hoist `step.*` fields to top level, remove nested `step` object entirely.
4. **Deterministic fallback** — normalize aliases, try serde, fail hard if invalid (no infinite synthesize loops).
5. **Tool output traversal** — already specced in `tool_outputs_traversal_spec.md`, implement those 5 tools.

---

## Phase 0: Flatten Schema + Remove Think (the loop-fix)

### Step 0.1 — Flatten `ControllerAction` enum

**File:** `orchestrator.rs` lines 1108–1155

Remove `ControllerStep` enum entirely. Hoist all step fields into `ControllerAction::NextStep`.

**Before:**
```rust
enum ControllerAction {
    NextStep { thinking: Value, step: ControllerStep },
    Complete { message: String },
    GuardrailStop { reason: String, message: Option<String> },
    AskUser { question: String, context: Option<String>, resume_to: ResumeTarget },
}

enum ControllerStep {
    Tool { description: String, tool: String, args: Value },
    Respond { description: String, message: String },
    Think { description: String },
    AskUser { description: String, question: String, context: Option<String>, resume_to: ResumeTarget },
}
```

**After:**
```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ControllerAction {
    NextStep {
        thinking: Value,
        // step type — required for next_step, inferred if missing
        #[serde(rename = "type")]
        step_type: Option<String>,
        description: Option<String>,
        // tool fields
        tool: Option<String>,
        #[serde(default)]
        args: Value,
        // respond fields
        message: Option<String>,
        // ask_user fields (when type=ask_user inside next_step)
        question: Option<String>,
        context: Option<String>,
        resume_to: Option<ResumeTarget>,
    },
    Complete {
        message: String,
    },
    GuardrailStop {
        reason: String,
        message: Option<String>,
    },
    AskUser {
        question: String,
        #[serde(default)]
        context: Option<String>,
        #[serde(default = "default_resume_target")]
        resume_to: ResumeTarget,
    },
}
```

Then add a post-parse validation method:

```rust
impl ControllerAction {
    fn validate(&self) -> Result<(), String> {
        match self {
            ControllerAction::NextStep { step_type, tool, message, question, .. } => {
                let effective_type = step_type.as_deref()
                    .or_else(|| infer_step_type_flat(tool, message, question));
                match effective_type {
                    Some("tool") => {
                        if tool.as_ref().map_or(true, |t| t.trim().is_empty()) {
                            return Err("next_step type=tool requires non-empty 'tool' field".into());
                        }
                    }
                    Some("respond") => {
                        if message.as_ref().map_or(true, |m| m.trim().is_empty()) {
                            return Err("next_step type=respond requires non-empty 'message' field".into());
                        }
                    }
                    Some("ask_user") => {
                        if question.as_ref().map_or(true, |q| q.trim().is_empty()) {
                            return Err("next_step type=ask_user requires non-empty 'question' field".into());
                        }
                    }
                    None => return Err("Cannot determine step type: provide 'type' or 'tool'/'message'/'question'".into()),
                    Some(other) => return Err(format!("Unknown step type: {other}")),
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
```

**Key:** `type` is inferred from presence of `tool` / `message` / `question` — no need for LLM to emit it explicitly. But we never synthesize a Think step.

### Step 0.2 — Flatten the JSON schema

**File:** `orchestrator.rs` lines 1569–1645

Replace `controller_output_format()`:

```rust
fn controller_output_format() -> Value {
    json_schema_output_format(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["action"],
        "properties": {
            "action": {
                "type": "string",
                "enum": ["next_step", "complete", "guardrail_stop", "ask_user"]
            },
            "thinking": {
                "type": "object",
                "properties": {
                    "task": { "type": "string" },
                    "facts": { "type": "array", "items": { "type": "string" } },
                    "decisions": { "type": "array", "items": { "type": "string" } },
                    "risks": { "type": "array", "items": { "type": "string" } },
                    "confidence": { "type": "number" }
                },
                "additionalProperties": true
            },
            "type": {
                "type": "string",
                "enum": ["tool", "respond", "ask_user"]
            },
            "description": { "type": "string" },
            "tool": { "type": "string" },
            "args": {},
            "message": { "type": "string" },
            "reason": { "type": "string" },
            "question": { "type": "string" },
            "context": { "type": "string" },
            "resume_to": {
                "type": "string",
                "enum": ["reflecting", "controller"]
            }
        },
        "additionalProperties": false
    }))
}
```

**Key changes:**
- No `oneOf`, no `allOf`, no `anyOf` — works identically on all providers
- No nested `step` object — `type`, `description`, `tool`, `args` are top-level
- No `"think"` in the `type` enum — removed entirely
- `args` has no type constraint (avoids `anyOf` for object|string)
- No `minimum`/`maximum` on confidence (Anthropic strips these anyway)

### Step 0.3 — Update the prompt

**File:** `prompts.rs` (lines 1-55) and `prompts/controller.txt` (lines 1-69)

Update `CONTROLLER_PROMPT_BASE` and `controller.txt` to reflect the flat schema:

1. Remove all mentions of `step` as a nested object
2. Remove `type="think"` from instructions
3. Show flat schema: `{ "action", "thinking", "type", "tool", "args", "description", "message", ... }`
4. Keep the instruction "If action is next_step, include a mandatory top-level 'thinking' object"
5. Remove "If you need to reason before tool use, choose next_step with type='think'" — replace with "Use the 'thinking' field to reason before any action. Do not output a separate think step."

### Step 0.4 — Remove `call_think` and Think execution path

**File:** `orchestrator.rs`

1. Delete `call_think` method (lines 876–889)
2. Remove `ControllerStep::Think` arm from `execute_step` (lines 328–339)
3. Delete `synthesize_step_from_thinking` (lines 1311–1329)
4. Delete `synthesize_tool_step_from_thinking` (lines 1331–1377) — was useful as last-resort fallback but creates loops
5. Remove `Think` from `StepAction` enum in `db/models/agent.rs` (if it exists there)
6. In `infer_step_type` (line 1496), remove the last fallback that returns `Some("think")` when only `description` is present — return `None` instead

### Step 0.5 — Rewrite `parse_controller_action`

**File:** `orchestrator.rs` lines 1177–1249

Replace the 73-line fallback cascade with a cleaner flow:

```rust
fn parse_controller_action(value: &Value) -> Result<ControllerAction, String> {
    // Step 1: Normalize aliases at the Value level before serde
    let normalized = normalize_controller_value(value);

    // Step 2: Try serde deserialization
    match serde_json::from_value::<ControllerAction>(normalized.clone()) {
        Ok(action) => {
            action.validate()?;
            Ok(action)
        }
        Err(serde_err) => {
            // Step 3: Handle action="respond" → Complete
            let action_str = normalized.get("action").and_then(|v| v.as_str());
            if action_str == Some("respond") {
                if let Some(msg) = non_empty_string_field(&normalized, &["message", "response"]) {
                    return Ok(ControllerAction::Complete { message: msg });
                }
            }

            // Step 4: Fail with clear error — no synthesis
            Err(format!("Invalid controller output: {serde_err}"))
        }
    }
}

fn normalize_controller_value(value: &Value) -> Value {
    let Value::Object(map) = value else { return value.clone() };
    let mut out = map.clone();

    // Hoist nested step fields to top level (backwards compat for LLMs that still nest)
    if let Some(Value::Object(step)) = out.remove("step").or_else(|| out.remove("next_step")) {
        for (key, val) in step {
            out.entry(key).or_insert(val);
        }
    }

    // Normalize tool name aliases
    for alias in ["tool_name", "name"] {
        if out.get("tool").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("tool".to_string(), val);
            }
        }
    }

    // Normalize args aliases
    for alias in ["tool_args", "arguments", "tool_input"] {
        if out.get("args").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("args".to_string(), val);
            }
        }
    }

    // Normalize message aliases
    for alias in ["response", "content"] {
        if out.get("message").is_none() {
            if let Some(val) = out.remove(alias) {
                out.insert("message".to_string(), val);
            }
        }
    }

    Value::Object(out)
}
```

### Step 0.6 — Update `execute_step` to work with flat `NextStep`

**File:** `orchestrator.rs` lines 209–415

Since `ControllerStep` is gone, `execute_step` now receives `ControllerAction::NextStep` directly. Refactor to extract fields from the flat struct:

```rust
// In the run() loop:
ControllerAction::NextStep { step_type, tool, args, message, question, context, resume_to, description, thinking } => {
    let effective_type = step_type.as_deref()
        .or_else(|| infer_step_type_flat(&tool, &message, &question))
        .unwrap(); // safe: validate() already checked

    self.ensure_plan(user_message)?;
    match self.execute_flat_step(call_llm, effective_type, description, tool, args, message, question, context, resume_to)? {
        StepExecutionOutcome::Continue => {}
        StepExecutionOutcome::Complete(response) => {
            return self.finish(response);
        }
    }
}
```

### Step 0.7 — Update tests

Update all existing tests to use flat schema shape. Key changes:
- Tests that check `ControllerStep::Think` should be removed or converted to test that think-like payloads are rejected
- `controller_output_schema_uses_one_of_for_action_requirements` → delete (no more oneOf)
- Add new test: flat tool payload without `step` wrapper parses correctly via serde (not fallback)
- Add new test: payload with only `thinking` and no `tool`/`message`/`question` returns `Err`, not a synthesized Think step
- Keep hydrate_tool_args tests (unchanged)

---

## Phase 1: Prompt Fix for Tool Output Persistence

### Step 1.1 — Improve the persist message shown to controller

**File:** `orchestrator.rs` lines 704–716

When output is persisted, the message currently says:
```
"Tool output stored in app data. Use tool_outputs.read to retrieve."
```

Change to include the traversal tools from the spec:
```json
{
    "persisted": true,
    "output_ref": { "id": "...", "storage": "app_data" },
    "size_chars": 12345,
    "preview": "...",
    "preview_truncated": true,
    "available_tools": [
        "tool_outputs.read — load full output into context",
        "tool_outputs.extract — extract fields via JSONPath",
        "tool_outputs.stats — get schema, field types, counts",
        "tool_outputs.list — list all stored outputs"
    ]
}
```

This gives the controller enough info to choose the right traversal tool instead of always calling `tool_outputs.read`.

### Step 1.2 — Auto-include metadata in persist message

When persisting, compute lightweight stats (key count, array lengths, top-level type) and include them in the preview. This way the controller can decide whether to extract specific paths vs read the whole thing.

---

## Phase 2: Implement Tool Output Traversal Suite

### Step 2.1 — Implement tools per existing spec

**Spec:** `src-tauri/docs/tool_outputs_traversal_spec.md`
**File:** `src-tauri/src/tools/tool_outputs.rs`

Implement the 5 tools in priority order per spec:
1. `tool_outputs.list` — list all stored outputs for current conversation
2. `tool_outputs.stats` — metadata, schema, field types, counts for a stored output
3. `tool_outputs.extract` — JSONPath extraction from stored output
4. `tool_outputs.count` — count matching elements
5. `tool_outputs.sample` — random sample from arrays

**Dependency:** `serde_json_path = "0.6"` in Cargo.toml (already noted in spec).

### Step 2.2 — Register traversal tools in ToolRegistry

Ensure all 5 tools are registered and appear in the controller's AVAILABLE TOOLS list.

### Step 2.3 — Update `hydrate_tool_args_for_execution`

Extend the existing hydration logic (currently handles `tool_outputs.read`) to also auto-hydrate `id` and `conversation_id` for all `tool_outputs.*` tools.

---

## Phase 3: Anthropic Schema Hardening

### Step 3.1 — Verify flat schema passes Anthropic validation

**File:** `llm/mod.rs` lines 146–205

The flat schema (Phase 0) has no `oneOf`/`anyOf`/`allOf`, so `strip_anthropic_unsupported_schema_keywords` becomes mostly a no-op for the controller schema. Verify:
- `additionalProperties: false` is set on root and `thinking` objects
- No numeric bounds on `confidence` (already handled — Anthropic strips them)
- `args` field has no type constraint (empty `{}` as schema) — verify Anthropic accepts this

### Step 3.2 — Add regression test

Add a test that passes the controller schema through `strip_anthropic_unsupported_schema_keywords` and verifies the output is structurally identical to the input (i.e., nothing is stripped because there's nothing unsupported).

---

## Execution Order

| Order | Step | Impact | Risk |
|-------|------|--------|------|
| 1 | 0.1–0.7 | **Fixes the infinite loop** | Medium — touches core loop, but well-scoped |
| 2 | 1.1–1.2 | Improves persistence UX | Low |
| 3 | 2.1–2.3 | Enables smart traversal | Low — additive, spec exists |
| 4 | 3.1–3.2 | Prevents regression | Low — verification only |

Phase 0 is the critical fix. Phases 1–3 are improvements that prevent the problem from recurring in different ways.

---

## Files Modified

| File | Phase | Changes |
|------|-------|---------|
| `src/agent/orchestrator.rs` | 0, 1 | Flatten enums, rewrite parser, remove Think, update persist message |
| `src/agent/prompts.rs` | 0 | Update `CONTROLLER_PROMPT_BASE` |
| `src/agent/prompts/controller.txt` | 0 | Update schema docs and instructions |
| `src/tools/tool_outputs.rs` | 2 | Implement 5 traversal tools |
| `src/llm/mod.rs` | 3 | Verify/test schema handling |
| `Cargo.toml` | 2 | Add `serde_json_path` dependency |

## Files NOT Modified

- `src/commands/agent.rs` — no changes needed, it just calls `controller.run()`
- `src/db/models/agent.rs` — only if `StepAction::Think` exists there (remove it)
