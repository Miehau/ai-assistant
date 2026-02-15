# Agent Engineering Guardrails

## Provider API Contract Changes
- Any provider-specific request/response contract change MUST include provider-specific tests in the same PR.
- Any provider-specific request/response contract change MUST include a local preflight payload check before network send.
- Shared schema changes MUST NOT be sent directly to provider adapters without provider-specific builders.

## Structured Output Schemas
- Use explicit provider schema builders:
  - `build_openai_output_schema(...)`
  - `build_anthropic_output_schema(...)`
- Anthropic requests MUST use only the Anthropic builder output.
- If Anthropic returns a schema-validation 4xx, add a regression test for the exact failing keyword/path before merging.

## PR Checklist (Required)
- [ ] Added/updated provider-specific tests for changed provider adapter behavior.
- [ ] Verified provider preflight validation catches unsupported payload shape.
- [ ] Verified no new `dead_code`/`unused` warnings from the change.
