# LLM Provider Dispatch Refactoring Plan

**Date:** 2026-03-10
**Scope:** Extract provider dispatch from `commands/agent.rs` into trait-based abstraction in `llm/`
**Risk Level:** Medium-High (touches the critical path of every LLM call)

---

## Executive Summary

The file `src-tauri/src/commands/agent.rs` currently contains **four** `match provider.as_str()` dispatch blocks that duplicate provider-specific logic for validation, controller calls, responder streaming, and title generation. Each block re-implements URL resolution, API key retrieval, system prompt injection, schema wrapping, and error handling. Meanwhile, the `src-tauri/src/llm/` module houses per-provider files that only expose low-level HTTP functions with no unifying interface.

This plan introduces a `LlmProvider` trait that encapsulates three capabilities -- controller call, streaming response, and simple completion -- and a factory that constructs the right provider from runtime config. The result: `agent.rs` calls `provider.controller_call(...)` without knowing which provider it talks to, and all provider-specific behavior lives inside `llm/`.

---

## Current State Analysis

### Match Block Inventory

| # | Location (approx. line) | Purpose | Branches |
|---|------------------------|---------|----------|
| 1 | ~542 | **Validation** -- check API key / custom backend exists | openai/anthropic/deepseek, custom, ollama/claude_cli, _ |
| 2 | ~744 (inside `call_llm` closure) | **Controller LLM call** -- structured output or tool use | openai, anthropic (2 sub-branches: tools vs. plain), deepseek, claude_cli, ollama, custom |
| 3 | ~1070 | **Responder streaming** | openai, anthropic, deepseek, custom/ollama, _ |
| 4 | ~1479 | **Title generation** (simple completion) | openai, anthropic, deepseek, claude_cli, custom/ollama |

### Provider-Specific Behaviors Leaking Into agent.rs

1. **System prompt injection strategy**
   - Anthropic + claude_cli: system prompt passed as separate parameter, NOT prepended to messages
   - OpenAI, DeepSeek, custom, Ollama: system prompt injected as first `{"role":"system",...}` message
   - This logic is duplicated in both the controller closure (~line 725) and the responder path (~line 1019)

2. **Controller output format**
   - Anthropic: uses native tool calling (`complete_anthropic_with_tools`) with `controller_tool_definitions()`, NOT structured output. Also swaps `CONTROLLER_PROMPT_BASE` for `CONTROLLER_PROMPT_ANTHROPIC`.
   - OpenAI/DeepSeek/custom: wraps the raw schema in a `json_schema` envelope with `name: "response"`, `strict: false`
   - Ollama: passes raw schema directly via `format` field
   - claude_cli: passes the raw schema to `format_claude_cli_prompt` which injects it as text instructions

3. **URL resolution**
   - OpenAI: hardcoded `https://api.openai.com/v1/chat/completions`
   - Anthropic: hardcoded inside `anthropic.rs` (`https://api.anthropic.com/v1/messages`)
   - DeepSeek: hardcoded `https://api.deepseek.com/chat/completions`
   - Ollama: hardcoded `http://localhost:11434/v1/chat/completions`
   - Custom: looked up from DB via `CustomBackendOperations::get_custom_backend_by_id`

4. **API key resolution**
   - Each provider reads its own key via `ModelOperations::get_api_key(&db, "<provider>")`
   - Custom backends have an optional key stored in the custom_backend record
   - Ollama and claude_cli need no key
   - Keys are fetched eagerly at worker thread start, then checked again inside each match arm

5. **Request options / caching**
   - `llm_request_options()` already dispatches by provider name to build caching config
   - Anthropic uses `anthropic_cache_breakpoints`; OpenAI uses `prompt_cache_key` / `prompt_cache_retention`
   - Others get default (empty) options

6. **Streaming support**
   - `supports_streaming()` returns false only for `claude_cli`
   - The responder path is entirely skipped for non-streaming providers

### The `call_llm` Closure Problem

The closure at ~line 713 captures by move/ref from the enclosing `catch_unwind` block:
- `provider: String`
- `model_for_thread: String`
- `conversation_id_for_thread: String`
- `assistant_message_id_for_thread: String`
- `controller_client: Client`
- `controller_request_options: LlmRequestOptions`
- `openai_api_key, anthropic_api_key, deepseek_api_key: String`
- `custom_backend_config: Option<(String, Option<String>)>`

Its signature is: `FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>`

The orchestrator calls it via `controller.run(user_message, &mut call_llm)` which propagates the closure through `call_controller` -> `call_llm_json`. The signature is constrained by the `F: FnMut(...)` generic on `DynamicController::run`.

---

## Identified Issues and Opportunities

### Critical

| ID | Issue | Impact |
|----|-------|--------|
| C1 | 4 duplicated provider match blocks in agent.rs | Every new provider requires changes in 4+ places; high merge-conflict risk |
| C2 | Anthropic controller path uses different prompts + tool calling, embedded in agent.rs | Anthropic-specific knowledge in the wrong layer |
| C3 | System prompt injection strategy duplicated twice | Bug risk: inconsistency between controller and responder prompt assembly |

### Major

| ID | Issue | Impact |
|----|-------|--------|
| M1 | API keys fetched eagerly then re-checked in each match arm | Redundant checks, potential for drift |
| M2 | URL strings hardcoded across agent.rs | Scattered magic strings; Ollama URL not configurable |
| M3 | Schema wrapping logic (json_schema envelope) duplicated for OpenAI, DeepSeek, custom | Missing for one provider = silent failure |
| M4 | `call_llm` closure captures 8+ variables from outer scope | Makes refactoring fragile, hard to test in isolation |
| M5 | `llm_request_options()` uses provider name string matching | Should be part of provider implementation |

### Minor

| ID | Issue | Impact |
|----|-------|--------|
| m1 | `supports_streaming()` is a standalone fn with string matching | Should be a trait method |
| m2 | Title generation re-fetches API keys from DB | Redundant if provider object already has credentials |
| m3 | Custom and Ollama share OpenAI-compatible codepath but are handled in separate match arms | Minor duplication |

---

## Proposed Trait Design

### Core Trait

```rust
/// Unified interface for LLM provider operations.
/// All provider-specific behavior (URL, auth, schema wrapping,
/// system prompt handling) is encapsulated in implementations.
pub trait LlmProvider: Send + Sync {
    /// Human-readable provider name (e.g., "openai", "anthropic")
    fn name(&self) -> &str;

    /// Whether this provider supports streaming responses.
    fn supports_streaming(&self) -> bool;

    /// Controller call: structured output or tool-use, depending on provider.
    ///
    /// - `messages`: conversation messages (without system prompt prepended)
    /// - `system_prompt`: optional system prompt (provider decides how to inject)
    /// - `output_format`: optional JSON schema for structured output
    /// - `request_options`: caching and other per-request config
    ///
    /// Returns the raw LLM response content + usage.
    fn controller_call(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        output_format: Option<Value>,
        request_options: Option<&LlmRequestOptions>,
    ) -> Result<StreamResult, String>;

    /// Streaming response (responder phase).
    /// Default implementation returns an error for non-streaming providers.
    fn stream_response(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
        request_options: Option<&LlmRequestOptions>,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<StreamResult, String> {
        let _ = (client, messages, system_prompt, request_options, on_chunk);
        Err(format!("Provider '{}' does not support streaming", self.name()))
    }

    /// Simple completion (used for title generation, triage, etc.)
    /// No structured output, no streaming.
    fn complete(
        &self,
        client: &Client,
        messages: &[LlmMessage],
        system_prompt: Option<&str>,
    ) -> Result<StreamResult, String>;

    /// Build request options appropriate for this provider.
    fn build_request_options(
        &self,
        conversation_id: &str,
        phase: &str,
    ) -> LlmRequestOptions;
}
```

### Provider Config Struct

```rust
/// Everything needed to construct a provider instance.
/// Resolved once at the start of a request, then passed around.
pub struct ProviderConfig {
    pub provider_name: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    // For custom backends
    pub custom_backend_id: Option<String>,
}
```

### Factory Function

```rust
/// Construct the appropriate LlmProvider from config.
/// Validates that required fields (api_key, url) are present.
pub fn create_provider(config: ProviderConfig) -> Result<Box<dyn LlmProvider>, String> {
    match config.provider_name.as_str() {
        "openai" => Ok(Box::new(OpenAiProvider::new(config)?)),
        "anthropic" => Ok(Box::new(AnthropicProvider::new(config)?)),
        "deepseek" => Ok(Box::new(DeepSeekProvider::new(config)?)),
        "ollama" => Ok(Box::new(OllamaProvider::new(config)?)),
        "claude_cli" => Ok(Box::new(ClaudeCliProvider::new(config)?)),
        "custom" => Ok(Box::new(CustomProvider::new(config)?)),
        other => Err(format!("Unsupported provider: {other}")),
    }
}
```

### How Each Provider Maps

| Provider | `controller_call` | `stream_response` | `complete` | System prompt | Notes |
|----------|-------------------|-------------------|------------|---------------|-------|
| **OpenAI** | `complete_openai_compatible_with_output_format_with_options` with json_schema envelope | `stream_openai_with_options` | `complete_openai` | Prepend as system message | URL hardcoded internally |
| **Anthropic** | If `output_format.is_some()`: swap prompt to `CONTROLLER_PROMPT_ANTHROPIC`, call `complete_anthropic_with_tools` with `controller_tool_definitions()`. Else: `complete_anthropic_with_output_format_with_options` | `stream_anthropic_with_options` | `complete_anthropic` | Pass as separate `system` param | Most complex; prompt swap + tool calling mode |
| **DeepSeek** | Same as OpenAI but with DeepSeek URL and key | `stream_openai_compatible_with_options` | `complete_openai_compatible` | Prepend as system message | Shares OpenAI-compatible codepath |
| **Ollama** | `complete_ollama_with_output_format_with_options` (raw schema in `format` field) | `stream_openai_compatible_with_options` | `complete_openai_compatible` | Prepend as system message | Localhost URL; no auth |
| **claude_cli** | `complete_claude_cli` with output_format passed through | N/A (streaming not supported) | `complete_claude_cli` | Separate `system` param | No HTTP client needed; uses CLI subprocess |
| **Custom** | Same as OpenAI but with custom URL and optional key; json_schema envelope | `stream_openai_compatible_with_options` | `complete_openai_compatible` | Prepend as system message | URL + key from DB |

### Anthropic Special Case (Detail)

The Anthropic controller path is the most unusual. When `output_format` is `Some(...)` (i.e., it is a controller call, not a simple completion), the current code:

1. Replaces any system message containing `CONTROLLER_PROMPT_BASE` with `CONTROLLER_PROMPT_ANTHROPIC`
2. Calls `complete_anthropic_with_tools(client, api_key, model, system, messages, controller_tool_definitions(), options)`
3. The `complete_anthropic_with_tools` function returns a `StreamResult` where `content` contains tool-use JSON that gets normalized via `tool_use_to_controller_json`

This behavior must live inside `AnthropicProvider::controller_call`. The provider will:
- Import `CONTROLLER_PROMPT_ANTHROPIC` and `controller_tool_definitions`
- Detect the controller mode from `output_format.is_some()`
- Replace the system prompt and use tool calling internally
- Return `StreamResult` with content that the orchestrator's `extract_json` + `parse_controller_action` can handle (it already does -- the current code flows through the same path)

---

## Proposed Refactoring Plan

### Phase 0: Preparation (Low Risk)

**Goal:** Add infrastructure without changing any behavior.

**Steps:**

1. **Create `src-tauri/src/llm/traits.rs`**
   - Define the `LlmProvider` trait (as above)
   - Define `ProviderConfig` struct
   - Define `create_provider` factory function (stub that returns `Err("not yet implemented")`)

2. **Create `src-tauri/src/llm/provider_utils.rs`**
   - Move `llm_request_options()` from `commands/agent.rs` into this file (renamed to `build_request_options_for_provider`)
   - Move `supports_streaming()` from `commands/agent.rs`
   - These become default/fallback implementations that provider impls can call
   - Keep the original functions in agent.rs as thin wrappers that delegate, so nothing breaks

3. **Update `src-tauri/src/llm/mod.rs`**
   - Add `mod traits;` and `mod provider_utils;`
   - Re-export `LlmProvider`, `ProviderConfig`, `create_provider`

4. **Verify compilation:** `cargo check`

**Files changed:** `llm/traits.rs` (new), `llm/provider_utils.rs` (new), `llm/mod.rs`
**Files NOT changed:** `commands/agent.rs`, `agent/orchestrator.rs`
**Risk:** Zero -- purely additive.

---

### Phase 1: Implement Provider Structs (Low Risk)

**Goal:** Each provider file gets a struct that implements `LlmProvider`, wrapping the existing free functions.

**Steps:**

1. **`src-tauri/src/llm/openai.rs`** -- Add `OpenAiProvider` struct
   - Fields: `api_key: String`, `model: String`
   - `controller_call`: wraps existing `complete_openai_compatible_with_output_format_with_options` with json_schema envelope logic
   - `stream_response`: wraps `stream_openai_with_options`
   - `complete`: wraps `complete_openai`
   - `build_request_options`: OpenAI-specific prompt cache key logic (moved from `llm_request_options`)
   - `supports_streaming`: returns `true`
   - Keep all existing free functions; the struct methods call them

2. **`src-tauri/src/llm/anthropic.rs`** -- Add `AnthropicProvider` struct
   - Fields: `api_key: String`, `model: String`
   - `controller_call`: contains the Anthropic-specific branching:
     - If `output_format.is_some()`: use `CONTROLLER_PROMPT_ANTHROPIC` + `complete_anthropic_with_tools`
     - Else: use `complete_anthropic_with_output_format_with_options`
   - `stream_response`: wraps `stream_anthropic_with_options`
   - `complete`: wraps `complete_anthropic`
   - `build_request_options`: anthropic cache breakpoints logic
   - Import `controller_tool_definitions`, `CONTROLLER_PROMPT_ANTHROPIC`, `CONTROLLER_PROMPT_BASE` from `agent::` modules

3. **`src-tauri/src/llm/ollama.rs`** -- Add `OllamaProvider` struct
   - Fields: `model: String`, `url: String` (default `http://localhost:11434/v1/chat/completions`)
   - `controller_call`: wraps `complete_ollama_with_output_format_with_options`
   - `stream_response`: wraps `stream_openai_compatible_with_options`
   - `complete`: wraps `complete_openai_compatible`

4. **`src-tauri/src/llm/claude_cli.rs`** -- Add `ClaudeCliProvider` struct
   - Fields: `model: String`
   - `controller_call`: wraps `complete_claude_cli`
   - `stream_response`: default impl (returns error)
   - `complete`: wraps `complete_claude_cli`
   - `supports_streaming`: returns `false`

5. **`src-tauri/src/llm/custom.rs`** (new file, or extend openai.rs with a `CustomProvider`)
   - Fields: `api_key: Option<String>`, `model: String`, `url: String`
   - Delegates to OpenAI-compatible functions
   - Alternatively, `CustomProvider` and `DeepSeekProvider` could wrap an `OpenAiCompatibleProvider` internally

6. **Implement `create_provider` factory** in `traits.rs`
   - Validate required fields (api_key present for openai/anthropic/deepseek; url present for custom)
   - Return the appropriate struct boxed as `dyn LlmProvider`

7. **Write unit tests for each provider's `controller_call`**
   - Test that OpenAI wraps schema in json_schema envelope
   - Test that Anthropic switches to tool calling when output_format is present
   - Test that Ollama passes raw schema
   - Test that claude_cli does not require a Client

**Files changed:** `llm/openai.rs`, `llm/anthropic.rs`, `llm/ollama.rs`, `llm/claude_cli.rs`, `llm/traits.rs`, optionally `llm/custom.rs` (new)
**Files NOT changed:** `commands/agent.rs`, `agent/orchestrator.rs`
**Risk:** Low -- purely additive. Existing free functions remain untouched. New code is testable in isolation.

---

### Phase 2: Wire Provider Into agent.rs Controller Path (Medium Risk)

**Goal:** Replace the `call_llm` closure's match block with a single `provider.controller_call(...)` call.

**Steps:**

1. **Resolve ProviderConfig at worker thread start**
   - Right after the provider validation match (~line 542), construct a `ProviderConfig` and call `create_provider(config)`
   - This replaces the eager API key fetches (`openai_api_key`, `anthropic_api_key`, `deepseek_api_key`) and the `custom_backend_config` resolution
   - The provider object now owns its credentials

2. **Replace the `call_llm` closure body**
   - Before (current): 120+ lines of match on provider name, system prompt injection, schema wrapping
   - After: ~15 lines:
     ```rust
     let mut call_llm = |messages: &[LlmMessage],
                          system_prompt: Option<&str>,
                          output_format: Option<Value>| {
         let result = provider.controller_call(
             &controller_client,
             messages,
             system_prompt,
             output_format,
             Some(&controller_request_options),
         );
         // ... elapsed timing + usage logging (keep as-is) ...
         result
     };
     ```
   - The system prompt prepend logic moves into each provider's `controller_call`

3. **Update `controller_request_options` construction**
   - Replace `llm_request_options(&provider, ...)` with `provider.build_request_options(conversation_id, "controller")`

4. **Remove dead API key variables**
   - `openai_api_key`, `anthropic_api_key`, `deepseek_api_key`, `custom_backend_config` no longer needed in the closure scope

5. **Remove the provider validation match block** (~line 542)
   - `create_provider` already validates credentials; if it returns `Err`, we bail early

6. **Verify:** `cargo check` then `cargo test`

**Files changed:** `commands/agent.rs`, `llm/mod.rs` (re-exports)
**Risk:** Medium. This is the first behavioral change. The closure signature stays the same (`FnMut(&[LlmMessage], Option<&str>, Option<Value>) -> Result<StreamResult, String>`) so the orchestrator is unaffected.

**Rollback:** Revert agent.rs changes only; the trait infrastructure remains for future use.

---

### Phase 3: Wire Provider Into Responder Streaming (Medium Risk)

**Goal:** Replace the responder streaming match block with `provider.stream_response(...)`.

**Steps:**

1. **Replace the system prompt injection for responder** (~line 1019)
   - Remove the `if provider == "anthropic" || provider == "claude_cli"` branch
   - The provider's `stream_response` handles this internally

2. **Replace the streaming match block** (~line 1070)
   - Before: ~65 lines of match with per-provider streaming calls
   - After:
     ```rust
     let stream_result = provider.stream_response(
         &stream_client,
         &responder_messages,  // no system message prepended
         responder_system_prompt,
         Some(&responder_request_options),
         &mut on_chunk,
     );
     ```

3. **Replace `supports_streaming(&provider)` call** (~line 985)
   - Use `provider.supports_streaming()`

4. **Update `responder_request_options`**
   - Use `provider.build_request_options(conversation_id, "responder")`

5. **Verify:** `cargo check` then `cargo test`

**Files changed:** `commands/agent.rs`
**Risk:** Medium. Same structural change as Phase 2 but for streaming path.

---

### Phase 4: Wire Provider Into Title Generation (Low-Medium Risk)

**Goal:** Replace the title generation match block with `provider.complete(...)`.

**Steps:**

1. **In the `generate_title` function** (~line 1479)
   - Construct provider from config (re-fetch from DB, or accept provider as parameter)
   - Replace the match block with `provider.complete(&client, &messages, Some(system_prompt))`
   - System prompt injection handled by provider internally

2. **Consider passing the provider object** to `generate_title` instead of re-constructing it
   - This avoids redundant DB lookups
   - But `generate_title` is called outside the main agent thread, so provider might need to be `Clone` or reconstructed

**Files changed:** `commands/agent.rs`
**Risk:** Low-Medium. Title generation is non-critical path.

---

### Phase 5: Cleanup (Low Risk)

**Goal:** Remove dead code and finalize the migration.

**Steps:**

1. **Remove the standalone `supports_streaming()` function** from agent.rs
2. **Remove `llm_request_options()` function** from agent.rs (now on trait)
3. **Remove per-provider API key extraction** that was only needed for the old match blocks
4. **Update the `cache_diagnostics_enabled()` function** -- consider making it a method on the provider or keeping it as-is (it uses the request options, not the provider directly)
5. **Remove unused imports** from agent.rs (`complete_anthropic`, `complete_openai`, etc. -- only the trait and factory are needed)
6. **Consider making free functions `pub(crate)` or `pub(super)`** in the provider files since external callers should use the trait
7. **Update `llm/mod.rs` re-exports** -- export only the trait, factory, and shared types
8. **Run `cargo test` and `bun run preflight`**

**Files changed:** `commands/agent.rs`, `llm/mod.rs`, all `llm/*.rs` files (visibility)
**Risk:** Low -- removing dead code.

---

### Phase 6 (Future / Optional): Refactor Orchestrator Signature

**Goal:** Replace the `call_llm` closure with a `&dyn LlmProvider` parameter.

This is a larger change to the orchestrator API that can be done later:

1. **Change `DynamicController::run` signature**
   - From: `run<F>(&mut self, user_message: &str, call_llm: &mut F) -> Result<String, String> where F: FnMut(...)`
   - To: `run(&mut self, user_message: &str, provider: &dyn LlmProvider, client: &Client, request_options: &LlmRequestOptions) -> Result<String, String>`

2. **Update `call_controller` and `call_llm_json`** to accept `&dyn LlmProvider` instead of `F`

3. **Move usage tracking / timing into the orchestrator** rather than the closure

This phase is optional because the closure adapter from Phase 2 already works. The benefit is cleaner code and testability (mock providers can implement the trait).

---

## File-by-File Change Summary

| File | Phase | Action |
|------|-------|--------|
| `src-tauri/src/llm/traits.rs` | 0, 1 | NEW -- trait definition, ProviderConfig, factory |
| `src-tauri/src/llm/provider_utils.rs` | 0 | NEW -- shared helpers (request options, system prompt prepend) |
| `src-tauri/src/llm/mod.rs` | 0, 1, 5 | MODIFY -- add mod declarations, update re-exports |
| `src-tauri/src/llm/openai.rs` | 1 | MODIFY -- add OpenAiProvider struct |
| `src-tauri/src/llm/anthropic.rs` | 1 | MODIFY -- add AnthropicProvider struct, import controller_tool_definitions |
| `src-tauri/src/llm/ollama.rs` | 1 | MODIFY -- add OllamaProvider struct |
| `src-tauri/src/llm/claude_cli.rs` | 1 | MODIFY -- add ClaudeCliProvider struct |
| `src-tauri/src/llm/custom.rs` | 1 | NEW (optional) -- or DeepSeekProvider/CustomProvider in openai.rs |
| `src-tauri/src/commands/agent.rs` | 2, 3, 4, 5 | MODIFY -- progressive replacement of match blocks |
| `src-tauri/src/agent/orchestrator.rs` | 6 (optional) | MODIFY -- change run() signature |

---

## Risk Assessment and Mitigation

### High-Impact Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Anthropic controller path regression** -- prompt swap or tool calling breaks | Controller fails to parse actions, agent hangs or errors | Phase 1: write integration-style test that calls `AnthropicProvider::controller_call` with mock, verify output is parseable by `parse_controller_action`. Keep `complete_anthropic_with_tools` untouched. |
| **System prompt double-injection** -- provider prepends system message AND caller also prepends it | LLM receives duplicate system instructions, confusing output | Phase 2: ensure `call_llm` closure NO LONGER prepends system messages. Only the provider does it. Add assertion test. |
| **Credential mismatch after refactor** -- wrong API key used for a provider | 401 errors from provider APIs | Factory validates at construction time. Add test that `create_provider` fails for openai without api_key. |

### Medium-Impact Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| **`call_llm` closure signature change** breaks orchestrator | Compilation error | Phase 2 keeps the closure signature IDENTICAL. The closure body changes, not the interface. |
| **DeepSeek or custom backend breaks** silently | Users of those providers get errors | Phase 2: test each provider's `controller_call` with known inputs. DeepSeek is structurally identical to OpenAI so risk is low. |
| **Claude CLI subprocess behavior changes** | Title generation or controller call fails | `ClaudeCliProvider` is a thin wrapper around the existing `complete_claude_cli` function -- no behavioral change. |

### Low-Impact Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Performance: extra allocation from `Box<dyn LlmProvider>`** | Negligible | One allocation per request, dwarfed by HTTP round-trip time. |
| **Merge conflicts** if other PRs touch agent.rs simultaneously | Delayed merge | Phase 0-1 do not touch agent.rs at all. Phases 2-4 can be separate PRs. |

---

## Testing Strategy

### Unit Tests (Phase 1)

- **Schema wrapping tests:** For each provider, verify that `controller_call` produces the correct request format when given a raw schema.
  - OpenAI: output_format wrapped in `{"type":"json_schema","json_schema":{"name":"response","strict":false,"schema":...}}`
  - Anthropic: output_format triggers tool calling mode (verify tool definitions are passed)
  - Ollama: output_format passed as raw `format` field
  - claude_cli: output_format passed through to `format_claude_cli_prompt`

- **System prompt injection tests:** For each provider, verify messages are prepared correctly.
  - OpenAI/DeepSeek/custom/Ollama: system message prepended
  - Anthropic/claude_cli: system NOT prepended (passed separately)

- **Validation tests:** `create_provider` returns Err for missing API keys/URLs.

- **Request options tests:** Each provider's `build_request_options` matches current `llm_request_options` output.

### Integration Tests (Phase 2-4)

- **Existing tests must pass:** The tests in `commands/agent.rs` and `llm/` modules (e.g., `llm_request_options_*`, `cache_diagnostics_*`) must continue to pass.
- **Schema validation tests:** `controller_schema_survives_anthropic_sanitizer_with_known_diff` and `controller_schema_passes_anthropic_validation` remain unchanged.

### Smoke Tests

- After Phase 2: manually test a controller loop with each provider (OpenAI, Anthropic at minimum).
- After Phase 3: manually test responder streaming with OpenAI and Anthropic.
- After Phase 4: verify title generation works.

### Regression Guardrails

- `bun run preflight` must pass at every phase boundary.
- Per `provider-contracts.md`: provider changes require preflight payload check and regression tests for 4xx errors.

---

## Success Metrics

1. **Zero `match provider.as_str()` blocks in `commands/agent.rs`** (excluding cache diagnostics which are purely observational)
2. **All provider-specific logic contained in `src-tauri/src/llm/`** directory
3. **Adding a new provider** requires: one new file in `llm/`, one match arm in `create_provider`, zero changes to `agent.rs` or `orchestrator.rs`
4. **Net reduction** of ~200-300 lines in `commands/agent.rs`
5. **All existing tests pass** with no modifications (except import path changes)
6. **`bun run preflight` green** at every phase boundary

---

## Migration Order Summary

```
Phase 0 (additive)     Phase 1 (additive)     Phase 2 (behavioral)
 traits.rs (new)   -->  Provider structs   -->  Replace call_llm closure body
 provider_utils.rs      in each llm/*.rs        in agent.rs
 mod.rs updates         + factory impl
                        + unit tests

Phase 3 (behavioral)   Phase 4 (behavioral)    Phase 5 (cleanup)
 Replace responder  -->  Replace title      -->  Remove dead code
 streaming match         generation match        Update visibility
 in agent.rs             in agent.rs             Final re-exports

                    Phase 6 (optional, future)
                     Refactor orchestrator
                     to accept &dyn LlmProvider
```

Each phase produces a compilable, testable, shippable state. Phases 2-4 can each be a separate PR.

---

## Appendix: Dependency Graph

```
commands/agent.rs
  |
  +-- [currently] imports 10+ free functions from llm/
  |
  +-- [after refactor] imports: create_provider, LlmProvider, ProviderConfig, LlmRequestOptions
  |
  +-- agent/orchestrator.rs
        |
        +-- uses call_llm closure (signature unchanged in Phases 0-5)
        +-- [Phase 6] accepts &dyn LlmProvider directly

llm/mod.rs
  +-- traits.rs        (LlmProvider trait, ProviderConfig, create_provider)
  +-- provider_utils.rs (shared helpers)
  +-- anthropic.rs     (AnthropicProvider + existing free functions)
  +-- openai.rs        (OpenAiProvider + DeepSeekProvider + existing free functions)
  +-- ollama.rs        (OllamaProvider + existing free functions)
  +-- claude_cli.rs    (ClaudeCliProvider + existing free functions)
  +-- custom.rs        (CustomProvider, optional)

agent/controller_parsing.rs
  +-- controller_tool_definitions()   <-- imported by AnthropicProvider
  +-- controller_output_format()      <-- unchanged

agent/prompts.rs
  +-- CONTROLLER_PROMPT_BASE          <-- unchanged
  +-- CONTROLLER_PROMPT_ANTHROPIC     <-- imported by AnthropicProvider
```
