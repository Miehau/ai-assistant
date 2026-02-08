# Tool Outputs Traversal Suite Specifications

## Overview
This document provides detailed specifications for 5 traversal tools to extend the tool_outputs system for efficient navigation and analysis of large JSON datasets stored from tool executions.

## Dependencies
Recommended JSONPath library: `serde_json_path` (RFC 9535 compliant)
Add to Cargo.toml: `serde_json_path = "0.6"`

---

## 1. tool_outputs.extract

### Purpose
Extract specific fields/paths from saved JSON output using JSONPath expressions, enabling targeted data retrieval without loading entire datasets.

### Metadata & Schema

```rust
ToolMetadata {
    name: "tool_outputs.extract".to_string(),
    description: "Extract specific fields from stored tool output using JSONPath expressions. Supports multiple paths and various output formats.".to_string(),
    args_schema: json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "The tool output reference ID"
            },
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Array of JSONPath expressions to extract",
                "minItems": 1
            },
            "flatten": {
                "type": "boolean",
                "default": false,
                "description": "Whether to flatten results into a single array"
            },
            "include_paths": {
                "type": "boolean",
                "default": false,
                "description": "Include the JSONPath expression with each result"
            },
            "default_value": {
                "description": "Default value for missing paths (null if not specified)"
            }
        },
        "required": ["id", "paths"],
        "additionalProperties": false
    }),
    result_schema: json!({
        "type": "object",
        "properties": {
            "extracted": {
                "description": "Extracted values, structure depends on flatten/include_paths options"
            },
            "missing_paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Paths that didn't match any values"
            }
        },
        "required": ["extracted"],
        "additionalProperties": false
    }),
    requires_approval: false,
    result_mode: ToolResultMode::Inline,
}
```

### Behavior
- Extracts multiple JSONPath expressions in a single call
- Returns structured results by default (object with path keys)
- Can flatten all results into a single array when `flatten=true`
- Handles missing paths gracefully with optional default values
- Supports complex JSONPath queries including filters and recursive descent

### Examples

```json
// Example 1: Extract specific fields from API response
{
  "id": "api-response-123",
  "paths": ["$.data.users[*].email", "$.data.users[*].name"],
  "flatten": false
}
// Returns: { "extracted": { "$.data.users[*].email": [...], "$.data.users[*].name": [...] } }

// Example 2: Extract with flattening
{
  "id": "dataset-456",
  "paths": ["$..price", "$..quantity"],
  "flatten": true
}
// Returns: { "extracted": [19.99, 29.99, 39.99, 2, 5, 1] }

// Example 3: Include path information
{
  "id": "config-789",
  "paths": ["$.servers[*].host", "$.servers[*].port"],
  "include_paths": true
}
// Returns: { "extracted": [{"path": "$.servers[*].host", "value": ["host1", "host2"]}, ...] }

// Example 4: Handle missing paths with defaults
{
  "id": "partial-data",
  "paths": ["$.name", "$.optional_field", "$.nested.value"],
  "default_value": "N/A"
}
// Returns: { "extracted": {"$.name": "John", "$.optional_field": "N/A", "$.nested.value": "N/A"}, "missing_paths": ["$.optional_field", "$.nested.value"] }

// Example 5: Complex JSONPath with filters
{
  "id": "products-list",
  "paths": ["$.products[?(@.price > 100)].name", "$.products[?(@.inStock == true)].id"]
}
// Returns filtered product names and IDs
```

### Error Cases
- Invalid output ID
- Malformed JSONPath expressions
- Output file not found or corrupted
- JSONPath compilation errors

### Implementation Notes
- Use `serde_json_path::JsonPath` for path compilation
- Cache compiled paths for repeated queries
- Stream processing for large files when possible
- Consider memory limits when extracting large arrays

### Testing Approach
- Unit tests for various JSONPath expressions
- Edge cases: empty results, deep nesting, large arrays
- Performance tests with files > 10MB
- Malformed path handling

---

## 2. tool_outputs.count

### Purpose
Count items or matches in saved output without loading entire datasets into memory, enabling efficient size analysis.

### Metadata & Schema

```rust
ToolMetadata {
    name: "tool_outputs.count".to_string(),
    description: "Count items in arrays, object keys, or matches without loading full data. Efficient for large datasets.".to_string(),
    args_schema: json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "The tool output reference ID"
            },
            "counts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name for this count operation"
                        },
                        "path": {
                            "type": "string",
                            "description": "JSONPath to the element to count"
                        },
                        "filter": {
                            "type": "string",
                            "description": "Optional JSONPath filter expression"
                        },
                        "count_type": {
                            "type": "string",
                            "enum": ["array_length", "object_keys", "matches", "nested_total"],
                            "default": "array_length",
                            "description": "Type of counting operation"
                        }
                    },
                    "required": ["name", "path"],
                    "additionalProperties": false
                },
                "minItems": 1,
                "description": "Array of count operations to perform"
            }
        },
        "required": ["id", "counts"],
        "additionalProperties": false
    }),
    result_schema: json!({
        "type": "object",
        "properties": {
            "counts": {
                "type": "object",
                "additionalProperties": { "type": "integer" }
            },
            "total": {
                "type": "integer",
                "description": "Sum of all counts"
            }
        },
        "required": ["counts"],
        "additionalProperties": false
    }),
    requires_approval: false,
    result_mode: ToolResultMode::Inline,
}
```

### Behavior
- Count array lengths at specific paths
- Count object keys
- Count matches for filter conditions
- Support multiple count operations in one call
- Efficient streaming for large files
- Return 0 for non-existent paths

### Examples

```json
// Example 1: Count array items
{
  "id": "users-data",
  "counts": [
    {"name": "total_users", "path": "$.users", "count_type": "array_length"},
    {"name": "active_users", "path": "$.users[?(@.active == true)]", "count_type": "matches"}
  ]
}
// Returns: { "counts": {"total_users": 150, "active_users": 89}, "total": 239 }

// Example 2: Count object keys
{
  "id": "config-file",
  "counts": [
    {"name": "root_keys", "path": "$", "count_type": "object_keys"},
    {"name": "server_configs", "path": "$.servers", "count_type": "object_keys"}
  ]
}
// Returns: { "counts": {"root_keys": 12, "server_configs": 5}, "total": 17 }

// Example 3: Count nested totals
{
  "id": "orders-db",
  "counts": [
    {"name": "all_items", "path": "$..items", "count_type": "nested_total"},
    {"name": "expensive_items", "path": "$..items[?(@.price > 100)]", "count_type": "matches"}
  ]
}
// Returns: { "counts": {"all_items": 523, "expensive_items": 47}, "total": 570 }

// Example 4: Multiple filter counts
{
  "id": "products",
  "counts": [
    {"name": "in_stock", "path": "$.products[?(@.stock > 0)]", "count_type": "matches"},
    {"name": "out_of_stock", "path": "$.products[?(@.stock == 0)]", "count_type": "matches"},
    {"name": "on_sale", "path": "$.products[?(@.sale == true)]", "count_type": "matches"}
  ]
}
// Returns categorized counts

// Example 5: Count at multiple paths
{
  "id": "api-responses",
  "counts": [
    {"name": "errors", "path": "$.errors", "count_type": "array_length"},
    {"name": "warnings", "path": "$.warnings", "count_type": "array_length"},
    {"name": "data_items", "path": "$.data.items", "count_type": "array_length"}
  ]
}
// Returns counts from different sections
```

### Error Cases
- Invalid output ID
- Malformed JSONPath expressions
- Non-countable elements (e.g., counting a string)
- Invalid count_type for the target

### Implementation Notes
- Use streaming JSON parser for large files
- Avoid loading entire arrays into memory
- Cache path resolutions
- Use efficient counting algorithms

### Testing Approach
- Test various data structures
- Large array counting (>100k items)
- Deep nesting scenarios
- Empty and missing path handling

---

## 3. tool_outputs.sample

### Purpose
Get random or systematic samples from large arrays without loading entire datasets, useful for data inspection and testing.

### Metadata & Schema

```rust
ToolMetadata {
    name: "tool_outputs.sample".to_string(),
    description: "Extract a sample of items from arrays in stored output. Supports random, systematic, and edge sampling strategies.".to_string(),
    args_schema: json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "The tool output reference ID"
            },
            "path": {
                "type": "string",
                "description": "JSONPath to the array to sample from"
            },
            "size": {
                "type": "integer",
                "minimum": 1,
                "maximum": 1000,
                "description": "Number of items to sample"
            },
            "strategy": {
                "type": "string",
                "enum": ["random", "first", "last", "systematic", "stratified"],
                "default": "random",
                "description": "Sampling strategy to use"
            },
            "seed": {
                "type": "integer",
                "description": "Random seed for reproducible sampling"
            },
            "stride": {
                "type": "integer",
                "minimum": 1,
                "description": "Step size for systematic sampling"
            },
            "strata_path": {
                "type": "string",
                "description": "JSONPath within items for stratified sampling"
            }
        },
        "required": ["id", "path", "size"],
        "additionalProperties": false
    }),
    result_schema: json!({
        "type": "object",
        "properties": {
            "sample": {
                "type": "array",
                "description": "Sampled items"
            },
            "total_items": {
                "type": "integer",
                "description": "Total number of items in source array"
            },
            "sample_size": {
                "type": "integer",
                "description": "Actual number of items sampled"
            },
            "indices": {
                "type": "array",
                "items": { "type": "integer" },
                "description": "Indices of sampled items in original array"
            }
        },
        "required": ["sample", "total_items", "sample_size"],
        "additionalProperties": false
    }),
    requires_approval: false,
    result_mode: ToolResultMode::Auto,
}
```

### Behavior
- Random sampling with optional seed for reproducibility
- First/last N items for edge inspection
- Systematic sampling with configurable stride
- Stratified sampling based on item properties
- Preserve original item structure
- Handle arrays smaller than sample size

### Examples

```json
// Example 1: Random sampling
{
  "id": "large-dataset",
  "path": "$.data.records",
  "size": 10,
  "strategy": "random",
  "seed": 42
}
// Returns: 10 randomly selected records with consistent results due to seed

// Example 2: First and last inspection
{
  "id": "time-series",
  "path": "$.measurements",
  "size": 5,
  "strategy": "first"
}
// Returns: First 5 measurements

// Example 3: Systematic sampling
{
  "id": "sensor-data",
  "path": "$.readings",
  "size": 100,
  "strategy": "systematic",
  "stride": 10
}
// Returns: Every 10th reading, up to 100 samples

// Example 4: Stratified sampling
{
  "id": "customer-data",
  "path": "$.customers",
  "size": 50,
  "strategy": "stratified",
  "strata_path": "$.region"
}
// Returns: Proportional sample from each region

// Example 5: Sample with indices
{
  "id": "products-list",
  "path": "$.products",
  "size": 20,
  "strategy": "random"
}
// Returns: { "sample": [...], "indices": [3, 7, 15, ...], "total_items": 500, "sample_size": 20 }
```

### Error Cases
- Path doesn't point to an array
- Sample size larger than array (returns full array with warning)
- Invalid strategy parameters
- Memory limits for large samples

### Implementation Notes
- Use reservoir sampling for random selection in streaming context
- Implement Fisher-Yates shuffle variant for random sampling
- Consider memory-efficient streaming for large arrays
- Cache array length for repeated sampling

### Testing Approach
- Statistical tests for random distribution
- Edge cases: empty arrays, single item, sample size > array size
- Performance with arrays > 1M items
- Reproducibility with seeds

---

## 4. tool_outputs.stats

### Purpose
Get comprehensive metadata and statistics about saved output without loading the entire file, useful for understanding data structure and content.

### Metadata & Schema

```rust
ToolMetadata {
    name: "tool_outputs.stats".to_string(),
    description: "Get statistics and metadata about stored tool output including size, structure, types, and optional schema generation.".to_string(),
    args_schema: json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "The tool output reference ID"
            },
            "include_schema": {
                "type": "boolean",
                "default": false,
                "description": "Generate and include JSON schema of the data"
            },
            "max_depth": {
                "type": "integer",
                "minimum": 1,
                "maximum": 10,
                "default": 5,
                "description": "Maximum depth to analyze"
            },
            "sample_arrays": {
                "type": "boolean",
                "default": true,
                "description": "Sample arrays to determine item types"
            },
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Specific paths to analyze (analyzes root if not specified)"
            }
        },
        "required": ["id"],
        "additionalProperties": false
    }),
    result_schema: json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "tool_name": { "type": "string" },
            "created_at": { "type": "integer" },
            "size": {
                "type": "object",
                "properties": {
                    "bytes": { "type": "integer" },
                    "characters": { "type": "integer" },
                    "formatted": { "type": "string" }
                }
            },
            "structure": {
                "type": "object",
                "properties": {
                    "root_type": { "type": "string" },
                    "max_depth": { "type": "integer" },
                    "total_keys": { "type": "integer" },
                    "total_values": { "type": "integer" }
                }
            },
            "types": {
                "type": "object",
                "additionalProperties": { "type": "integer" },
                "description": "Count of each JSON type"
            },
            "arrays": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "length": { "type": "integer" },
                        "item_type": { "type": "string" }
                    }
                }
            },
            "objects": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "keys": { "type": "integer" }
                    }
                }
            },
            "schema": {
                "description": "Generated JSON schema if requested"
            }
        },
        "required": ["id", "size", "structure", "types"],
        "additionalProperties": false
    }),
    requires_approval: false,
    result_mode: ToolResultMode::Inline,
}
```

### Behavior
- Analyze file without full loading
- Count types at all levels
- Identify arrays with lengths and item types
- Track object key counts
- Generate JSON schema on demand
- Provide human-readable size formatting

### Examples

```json
// Example 1: Basic statistics
{
  "id": "api-response-data",
  "include_schema": false
}
// Returns: Full stats without schema

// Example 2: With schema generation
{
  "id": "config-file",
  "include_schema": true,
  "max_depth": 3
}
// Returns: Stats plus inferred JSON schema up to depth 3

// Example 3: Analyze specific paths
{
  "id": "nested-data",
  "paths": ["$.users", "$.products", "$.orders"],
  "sample_arrays": true
}
// Returns: Stats for specified paths only

// Example 4: Deep structure analysis
{
  "id": "complex-json",
  "max_depth": 10,
  "include_schema": true,
  "sample_arrays": true
}
// Returns: Comprehensive analysis with deep traversal

// Example 5: Quick overview
{
  "id": "large-file",
  "max_depth": 2,
  "sample_arrays": false,
  "include_schema": false
}
// Returns: Fast surface-level statistics
```

### Return Example
```json
{
  "id": "api-response-123",
  "tool_name": "web.fetch",
  "created_at": 1700000000000,
  "size": {
    "bytes": 524288,
    "characters": 524288,
    "formatted": "512 KB"
  },
  "structure": {
    "root_type": "object",
    "max_depth": 5,
    "total_keys": 145,
    "total_values": 3420
  },
  "types": {
    "object": 45,
    "array": 23,
    "string": 2140,
    "number": 890,
    "boolean": 322,
    "null": 0
  },
  "arrays": [
    {"path": "$.users", "length": 150, "item_type": "object"},
    {"path": "$.products", "length": 89, "item_type": "object"}
  ],
  "objects": [
    {"path": "$", "keys": 8},
    {"path": "$.metadata", "keys": 12}
  ],
  "schema": { /* Generated JSON Schema */ }
}
```

### Error Cases
- Invalid output ID
- Corrupted JSON file
- Memory limits for schema generation
- Circular reference detection

### Implementation Notes
- Use streaming parser for size/type counting
- Sample arrays intelligently (first, middle, last items)
- Implement incremental schema builder
- Cache results for repeated queries

### Testing Approach
- Various JSON structures (deep, wide, mixed)
- Large file handling (>100MB)
- Schema generation accuracy
- Performance benchmarks

---

## 5. tool_outputs.list

### Purpose
List all available saved outputs with filtering and metadata, enabling discovery and management of stored tool results.

### Metadata & Schema

```rust
ToolMetadata {
    name: "tool_outputs.list".to_string(),
    description: "List stored tool outputs with filtering, sorting, and preview capabilities.".to_string(),
    args_schema: json!({
        "type": "object",
        "properties": {
            "conversation_id": {
                "type": "string",
                "description": "Filter by conversation ID"
            },
            "tool_name": {
                "type": "string",
                "description": "Filter by tool name"
            },
            "success": {
                "type": "boolean",
                "description": "Filter by success status"
            },
            "after": {
                "type": "integer",
                "description": "Unix timestamp - only show outputs created after this time"
            },
            "before": {
                "type": "integer",
                "description": "Unix timestamp - only show outputs created before this time"
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "default": 20,
                "description": "Maximum number of results"
            },
            "offset": {
                "type": "integer",
                "minimum": 0,
                "default": 0,
                "description": "Number of results to skip"
            },
            "sort_by": {
                "type": "string",
                "enum": ["created_at", "size", "tool_name"],
                "default": "created_at",
                "description": "Sort field"
            },
            "sort_order": {
                "type": "string",
                "enum": ["asc", "desc"],
                "default": "desc",
                "description": "Sort order"
            },
            "include_preview": {
                "type": "boolean",
                "default": true,
                "description": "Include preview of output data"
            },
            "preview_length": {
                "type": "integer",
                "minimum": 0,
                "maximum": 500,
                "default": 100,
                "description": "Characters to include in preview"
            }
        },
        "additionalProperties": false
    }),
    result_schema: json!({
        "type": "object",
        "properties": {
            "outputs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "tool_name": { "type": "string" },
                        "conversation_id": { "type": ["string", "null"] },
                        "message_id": { "type": "string" },
                        "created_at": { "type": "integer" },
                        "success": { "type": "boolean" },
                        "size_bytes": { "type": "integer" },
                        "preview": { "type": "string" },
                        "summary": {
                            "type": "object",
                            "properties": {
                                "type": { "type": "string" },
                                "keys": { "type": "integer" },
                                "items": { "type": "integer" }
                            }
                        }
                    },
                    "required": ["id", "tool_name", "created_at", "success", "size_bytes"]
                }
            },
            "total": {
                "type": "integer",
                "description": "Total matching outputs"
            },
            "has_more": {
                "type": "boolean",
                "description": "Whether more results exist"
            }
        },
        "required": ["outputs", "total", "has_more"],
        "additionalProperties": false
    }),
    requires_approval: false,
    result_mode: ToolResultMode::Inline,
}
```

### Behavior
- List outputs from filesystem storage
- Apply multiple filters simultaneously
- Sort by various fields
- Paginate results
- Generate previews without loading full files
- Provide quick summaries of data structure

### Examples

```json
// Example 1: List recent outputs
{
  "limit": 10,
  "sort_by": "created_at",
  "sort_order": "desc",
  "include_preview": true
}
// Returns: 10 most recent outputs with previews

// Example 2: Filter by conversation
{
  "conversation_id": "conv-123",
  "tool_name": "web.fetch",
  "success": true,
  "limit": 20
}
// Returns: Successful web.fetch outputs from specific conversation

// Example 3: Date range filtering
{
  "after": 1700000000,
  "before": 1700086400,
  "sort_by": "size",
  "sort_order": "desc"
}
// Returns: Outputs from specific day, sorted by size

// Example 4: Paginated results
{
  "limit": 20,
  "offset": 40,
  "tool_name": "search.rg",
  "include_preview": false
}
// Returns: Page 3 of search results (items 41-60)

// Example 5: Failed operations audit
{
  "success": false,
  "limit": 50,
  "sort_by": "created_at",
  "sort_order": "desc",
  "preview_length": 200
}
// Returns: Recent failed operations with extended previews
```

### Return Example
```json
{
  "outputs": [
    {
      "id": "output-abc123",
      "tool_name": "web.fetch",
      "conversation_id": "conv-456",
      "message_id": "msg-789",
      "created_at": 1700000000000,
      "success": true,
      "size_bytes": 45678,
      "preview": "{\"status\":200,\"data\":{\"users\":[...]}",
      "summary": {
        "type": "object",
        "keys": 4,
        "items": 150
      }
    }
  ],
  "total": 234,
  "has_more": true
}
```

### Error Cases
- Invalid filter combinations
- Filesystem access errors
- Corrupted metadata files
- Invalid sort parameters

### Implementation Notes
- Index outputs in memory or lightweight DB for fast queries
- Lazy loading of file metadata
- Efficient preview generation (read first N bytes)
- Consider caching directory listings

### Testing Approach
- Large directory handling (>1000 files)
- Complex filter combinations
- Pagination edge cases
- Sort performance

---

## General Considerations

### Consistency
- All tools follow similar argument patterns with `id` as primary identifier
- Consistent error handling with descriptive messages
- Similar result structures with metadata included
- Use of JSONPath for all path-based operations

### Composition
- Results from `list` provide IDs for other operations
- `stats` output helps determine parameters for `extract` and `sample`
- `count` results inform `sample` size decisions
- Tools can be chained via output IDs

### Performance
- **Streaming**: `count` and `stats` use streaming parsers
- **Sampling**: `sample` uses reservoir sampling for memory efficiency
- **Caching**: Compiled JSONPaths and metadata cached
- **Lazy Loading**: `list` doesn't load file contents
- **Partial Reading**: Preview generation reads only needed bytes

### Error Handling
Common error cases across all tools:
- Invalid output ID format
- File not found
- Corrupted JSON
- Invalid JSONPath expressions
- Memory limit exceeded
- Filesystem permission errors

Error messages include:
- Tool name
- Operation attempted
- Specific failure reason
- Suggestions for resolution

### Testing Strategy
1. **Unit Tests**
   - Individual function testing
   - JSONPath compilation and execution
   - Error handling paths

2. **Integration Tests**
   - Full tool execution flows
   - Multi-tool workflows
   - File system interactions

3. **Performance Tests**
   - Large file handling (>100MB)
   - Many small files (>1000)
   - Complex JSONPath queries
   - Memory usage monitoring

4. **Edge Cases**
   - Empty files
   - Deeply nested structures
   - Circular references
   - Unicode and special characters
   - Concurrent access

### Implementation Priority
1. `tool_outputs.list` - Foundation for discovery
2. `tool_outputs.stats` - Understanding data structure
3. `tool_outputs.extract` - Core data retrieval
4. `tool_outputs.count` - Efficient size analysis
5. `tool_outputs.sample` - Data inspection capability

### Future Enhancements
- Query language beyond JSONPath
- Aggregation operations (sum, avg, min, max)
- Transformation capabilities
- Export to different formats
- Compression support
- Distributed storage backends