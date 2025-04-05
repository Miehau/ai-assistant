# Rust Migration Plan: Moving File and Attachment Processing

## Current Architecture

The AI-Frontend application currently uses a hybrid architecture:

- **Frontend (Svelte/TypeScript)**: Handles UI, user interactions, and file processing
- **Backend (Rust/Tauri)**: Manages database operations, models, API keys, and conversation history

Currently, file attachments are processed entirely on the frontend:
- Files are read and converted to base64
- Attachment metadata is stored alongside the base64 data
- All attachment data is stored in the database as part of message objects

## Functionalities to Migrate

### 1. File Upload and Processing

**Current Implementation (Frontend)**:
- File selection via input element
- Reading files with FileReader API
- Converting to base64
- Detecting file types
- Creating attachment objects with inline data

**Target Implementation (Rust)**:
- File selection remains on frontend
- File data sent to Rust backend
- Rust handles saving files to disk
- File metadata returned to frontend
- References to files stored in database instead of raw data

### 2. File Storage and Retrieval

**Current Implementation (Frontend)**:
- Base64 data stored directly in database
- Frontend decodes and displays data

**Target Implementation (Rust)**:
- Files stored on disk in organized directory structure
- Database stores file paths and metadata
- Rust provides APIs to retrieve files when needed
- Frontend requests file data only when needed for display

### 3. Attachment Type Handling

**Current Implementation (Frontend)**:
- Simple MIME type detection
- Basic handling of image, audio, and text files

**Target Implementation (Rust)**:
- More robust MIME type detection
- Enhanced file validation
- Specialized processing for different file types
- Potential for file conversion/optimization

### 4. Audio Transcription

**Current Implementation (Frontend)**:
- Limited or no transcription capabilities

**Target Implementation (Rust)**:
- Integration with audio processing libraries
- Local transcription capabilities
- Metadata extraction from audio files

### 5. Image Processing

**Current Implementation (Frontend)**:
- Basic image display
- No processing or optimization

**Target Implementation (Rust)**:
- Image resizing and optimization
- Thumbnail generation
- Metadata extraction (EXIF, etc.)
- Potential for basic image analysis

### 6. Text File Processing

**Current Implementation (Frontend)**:
- Basic text display
- Limited parsing capabilities

**Target Implementation (Rust)**:
- Enhanced text parsing
- Support for various formats (markdown, code, etc.)
- Content extraction from structured documents

## Implementation Plan

### Phase 1: Foundation

1. **Create File Management Module** ✅
   - Implement basic file system operations ✅
   - Create directory structure for attachment storage ✅
   - Develop file path management utilities ✅

2. **Update Database Schema** ✅
   - Add file paths and metadata fields ✅
   - Modify attachment schema to reference files instead of storing data ✅
   - Create migration path for existing attachments ✅

3. **Implement Core Tauri Commands** ✅
   - File upload handler ✅
   - File retrieval handler ✅
   - File deletion handler ✅

### Phase 2: Enhanced File Processing

1. **Image Processing** ✅
   - Implement image validation and sanitization ✅
   - Add thumbnail generation ✅
   - Develop image optimization ✅

2. **Audio Processing** ✅
   - Implement audio file validation ✅
   - Add basic audio metadata extraction ✅
   - Explore transcription options ✅

3. **Text Processing** ✅
   - Implement text file validation ✅
   - Add content extraction and parsing ✅
   - Support for code syntax highlighting ✅

### Phase 3: Frontend Integration

1. **Update Frontend API** *(In Progress)*
   - Modify attachment handling to use new Rust commands
   - Update UI components to work with file references
   - Implement progressive loading for large files

2. **Enhance User Experience**
   - Add upload progress indicators
   - Implement drag-and-drop functionality
   - Add file previews and thumbnails

3. **Migration Utilities**
   - Create tools to migrate existing attachments
   - Ensure backward compatibility

### Phase 4: Advanced Features

1. **File Versioning**
   - Implement file versioning system
   - Add version history UI

2. **Enhanced Security**
   - Implement file encryption
   - Add access control for sensitive files

3. **Optimization**
   - Implement caching strategies
   - Add compression for storage efficiency

## Technical Considerations

### File Storage Strategy

Files will be stored in a structured directory:
```
app_data_dir/
├── attachments/
│   ├── [conversation_id]/
│   │   ├── [message_id]/
│   │   │   ├── [attachment_id].[extension]
│   │   │   ├── [attachment_id].thumbnail.[extension]
│   │   │   └── [attachment_id].metadata.json
```

### Database Schema Changes

The attachment schema will be updated to:
```rust
struct Attachment {
    id: String,
    message_id: String,
    name: String,
    file_path: String,
    attachment_type: String,
    mime_type: String,
    size_bytes: u64,
    created_at: DateTime,
    metadata: Option<String>, // JSON string with additional metadata
}
```

### API Design

New Tauri commands will include:
- `upload_attachment(file_data: Vec<u8>, file_name: String, mime_type: String) -> AttachmentMetadata`
- `get_attachment(attachment_id: String) -> Vec<u8>`
- `get_attachment_thumbnail(attachment_id: String) -> Vec<u8>`
- `delete_attachment(attachment_id: String) -> bool`

## Migration Strategy

1. **Incremental Approach**:
   - Implement new functionality alongside existing code
   - Gradually migrate features one by one
   - Maintain backward compatibility

2. **Data Migration**:
   - Create a one-time migration utility for existing attachments
   - Convert base64 data to files on disk
   - Update database references

3. **Testing Strategy**:
   - Unit tests for Rust file operations
   - Integration tests for frontend-backend communication
   - End-to-end tests for complete attachment workflows
