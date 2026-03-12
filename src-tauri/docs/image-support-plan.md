# Image Support Implementation Plan

## Phase 1: Backend Foundation
- [x] Extend existing `files.read` tool with `as_type` parameter
  - Add `as_type` enum: "text" (default), "base64"
  - Detect media type from file extension for base64 mode
  - Use `fs::read` + base64 encoding for binary files
  - Separate size limits: 64KB for text, 5MB for base64
  - Return `media_type` field when as_type="base64"
  - Maintain backward compatibility (as_type defaults to "text")
- [ ] Create message content builder helper that constructs multi-part content arrays
  - Accept text + optional file paths
  - Build proper content block structure with image blocks

## Phase 2: Anthropic Provider Updates
- [ ] Update `format_anthropic_messages` to preserve content arrays
  - Currently uses `value_to_string` which strips structure
  - Pass through image blocks directly to API
- [ ] Update `format_anthropic_system` if needed for images in system context
- [ ] Add integration test with mock image data

## Phase 3: Message Construction
- [ ] Update orchestrator message building to support attachments
  - Accept file paths in user input
  - Build multi-part content using helper
- [ ] Update IPC command for sending messages to accept optional file paths array
  - Modify existing send message command signature
  - Validate files exist before processing

## Phase 4: Frontend Integration
- [ ] Add file input UI to chat interface
  - File picker button in message input area
  - Image preview thumbnails
  - Remove attachment button
- [ ] Update chat service to pass file paths via IPC
- [ ] Display images in message history
  - Show images inline in chat bubbles
  - Support both sent and received image references

## Phase 5: Testing & Polish
- [ ] Test with various image formats (JPEG, PNG, WebP)
- [ ] Test file size limits and error handling
- [ ] Test with multiple images in one message
- [ ] Add user-facing error messages for invalid files
- [ ] Update documentation

## Phase 6: OpenAI Support (Optional)
- [ ] Add OpenAI Vision format conversion
- [ ] Update OpenAI provider to handle image content blocks
- [ ] Test with OpenAI models that support vision
