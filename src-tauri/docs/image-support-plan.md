# Image Support Implementation Plan

## Content block format (provider-neutral)

`LlmMessage.content` may be a plain string (existing) or a JSON array of blocks:

```json
[
  { "type": "image", "media_type": "image/jpeg", "data": "<base64>" },
  { "type": "text",  "text": "user message here" }
]
```

Each provider's formatter is responsible for mapping this to its own API shape.

---

## Phase 1: Backend Foundation
- [x] Extend existing `files.read` tool with `as_type` parameter
  - Add `as_type` enum: "text" (default), "base64"
  - Detect media type from file extension for base64 mode
  - Use `fs::read` + base64 encoding for binary files
  - Separate size limits: 64KB for text, 5MB for base64
  - Return `media_type` field when as_type="base64"
  - Maintain backward compatibility (as_type defaults to "text")
- [x] Update `build_user_content` in `commands/agent.rs` to emit provider-neutral blocks
  - Replace OpenAI-specific `image_url` blocks with `{type, media_type, data}` blocks
  - Text and non-image attachments continue to be inlined into the text block
  - Added per-image `<attached_image filename="…" type="…" />` XML annotation in text block
  - Images only sent as binary blocks on the originating turn (data URI path); history turns keep only the XML annotation

## Phase 2: Provider Updates
- [x] **Anthropic** (`llm/anthropic.rs`) — `format_anthropic_messages`
  - Added `parse_content_blocks` + `ContentBlock` enum in `llm/mod.rs` (shared helper)
  - Map neutral image blocks → `{"type":"image","source":{"type":"base64","media_type":"...","data":"..."}}`
  - Only apply `split_anthropic_text_for_cache` to text blocks; image blocks passed through as-is
  - `cache_control` tracks last **text** block index — image blocks never receive it; image-only messages excluded from cache marking entirely
  - Tests: image block mapping, cache_control on text not image, image-only message skipped
- [ ] **OpenAI / Ollama** (`llm/openai.rs`) — `build_openai_compatible_body`
  - Map neutral image blocks → `{"type":"image_url","image_url":{"url":"data:<media_type>;base64,<data>","detail":"auto"}}`
- [ ] **Claude CLI** (`llm/claude_cli.rs`) — `format_claude_cli_prompt`
  - CLI is text-only; log a warning and skip image blocks
- [ ] **Update `format_anthropic_system`** if images ever appear in system context

## Phase 3: Frontend Integration
- [ ] Add file input UI to chat interface
  - File picker button in message input area
  - Image preview thumbnails
  - Remove attachment button
- [ ] Update chat service to pass attachments via IPC (already partially wired)
- [ ] Display images inline in message history (sent and received)

## Phase 4: Testing & Polish
- [ ] Test with various image formats (JPEG, PNG, WebP)
- [ ] Test file size limits and error handling
- [ ] Test with multiple images in one message
- [ ] Add user-facing error messages for invalid files
- [ ] Update documentation
