# Dead Code and Cleanup

## Overview

The codebase contains ~650 lines of dead code and 6 unused dependencies that can be safely removed.

---

## Files to Delete

### Unused Components

| File | Lines | Reason |
|------|-------|--------|
| `src/lib/components/RecipeModal.svelte` | 93 | Never imported anywhere |
| `src/lib/components/AddRecipeModal.svelte` | 113 | Never imported anywhere |

**Evidence**: Grep search for "RecipeModal" and "AddRecipeModal" returns only the file definitions, no imports.

### Unused Services

| File | Lines | Reason |
|------|-------|--------|
| `src/lib/services/qdrant.ts` | 78 | QdrantService never instantiated |
| `src/lib/services/LangfuseService.ts` | 81 | Service never used |
| `src/lib/services/api.ts` | 83 | Legacy HTTP client, replaced by Tauri |

**Evidence**:
- `qdrant.ts` exports `QdrantService` but no file imports it
- `LangfuseService.ts` exports class but no file imports it
- `api.ts` points to `localhost:3000` which is no longer used

### Duplicate Services

| File | Lines | Reason |
|------|-------|--------|
| `src/lib/models/apiKeyService.ts` | 115 | Duplicate of `.svelte.ts` version |

**Evidence**: Both files exist:
- `apiKeyService.ts` - Class-based
- `apiKeyService.svelte.ts` - Svelte 5 runes version

The `.svelte.ts` version is newer and should be kept.

### Unused Routes

| Directory | Lines | Reason |
|-----------|-------|--------|
| `src/routes/cv/` | 87 | Orphan route, not linked from navigation |

**Evidence**: No internal links to `/cv` route found.

### Deprecated Type Files

| File | Reason |
|------|--------|
| `src/lib/types/index.ts` | Marked as deprecated in file, re-exports only |

---

## Dependencies to Remove

### package.json

```json
{
  "dependencies": {
    "pouchdb": "^9.0.0",              // 0 imports found
    "@types/pouchdb": "^6.4.2",       // 0 imports found
    "events": "^3.3.0",               // 0 imports found
    "@qdrant/js-client-rest": "^1.7.0", // Only in unused qdrant.ts
    "langfuse": "^3.30.1",            // Only in unused LangfuseService.ts
    "svelte-ux": "^1.0.9"             // 0 imports found
  }
}
```

### Estimated Savings

| Dependency | Size Impact |
|------------|-------------|
| pouchdb | ~15MB |
| @qdrant/js-client-rest | ~5MB |
| langfuse | ~3MB |
| events | ~100KB |
| svelte-ux | ~2MB |
| **Total** | **~25-30MB** |

---

## Questionable Dependencies

### @langchain/openai

**Current usage**: Used in `openai.ts` for `ChatOpenAI` class, but same file also uses direct `fetch()` to OpenAI API.

**Question**: Do you need both LangChain wrapper AND direct API calls?

**Options**:
1. Remove `@langchain/openai`, use direct API consistently
2. Use LangChain consistently, remove direct fetch calls

### Form Libraries

Both installed:
- `formsnap` - 12 files reference it
- `sveltekit-superforms` - 6 files reference it

**Recommendation**: Pick one and standardize.

---

## Cleanup Commands

### Delete Dead Files

```bash
# Components
rm src/lib/components/RecipeModal.svelte
rm src/lib/components/AddRecipeModal.svelte

# Services
rm src/lib/services/qdrant.ts
rm src/lib/services/LangfuseService.ts
rm src/lib/services/api.ts
rm src/lib/models/apiKeyService.ts

# Routes
rm -rf src/routes/cv

# Types
rm src/lib/types/index.ts
```

### Remove Dependencies

```bash
# Remove unused packages
bun remove pouchdb @types/pouchdb events @qdrant/js-client-rest langfuse svelte-ux
```

### Verify Removal

```bash
# Check for broken imports after cleanup
bun run build

# Or with TypeScript check
npx tsc --noEmit
```

---

## Database Tables to Audit

**Location**: `src-tauri/src/db/mod.rs`

Potentially unused tables (no commands reference them):

| Table | Lines | Status |
|-------|-------|--------|
| `users` | 41-45 | Never queried |
| `memories` | 46-50 | Never queried |
| `message_tool_executions` | - | Agent feature (check if used) |
| `message_agent_thinking` | - | Agent feature (check if used) |

**Recommendation**: Audit actual usage before removing migrations.

---

## Code to Refactor (Not Delete)

### Duplicate Command Registration

**File**: `src-tauri/src/main.rs:31-103`

Commands registered twice:
- `get_file` (lines 36 and 62)
- `delete_file` (lines 39 and 63)
- `get_image_thumbnail` (lines 37 and 66)
- `optimize_image` (lines 38 and 67)

**Fix**: Remove duplicates, organize by feature.

### Type Aliases to Remove

**File**: `src/lib/types.ts`

```typescript
// These aliases add confusion, use canonical names instead
export type Message = DisplayMessage;       // Remove
export type MessageWithTree = MessageWithBranch;  // Remove
```

### Re-exports to Simplify

**File**: `src/lib/types.ts`

Large file that mostly re-exports from `types/` subdirectory. Consider:
1. Import directly from `$lib/types/message` etc.
2. Or keep single entry point but make it cleaner

---

## Summary

### Immediate Cleanup (< 1 hour)

| Action | Impact |
|--------|--------|
| Delete 6 unused files | -550 lines |
| Remove 6 dependencies | -25MB node_modules |
| Delete duplicate service | -115 lines |
| Delete orphan route | -87 lines |
| **Total** | **~650 lines, ~25MB** |

### Follow-up Cleanup

| Action | Impact |
|--------|--------|
| Fix duplicate command registration | Cleaner main.rs |
| Remove type aliases | Clearer type system |
| Audit database tables | Smaller DB schema |
| Choose one form library | Consistent patterns |
