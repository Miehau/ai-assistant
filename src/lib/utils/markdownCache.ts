/**
 * Global LRU cache for parsed markdown content.
 * Prevents redundant parsing on component remounts and improves performance.
 */

const MAX_CACHE_SIZE = 500;
const parseCache = new Map<string, string>();

/**
 * Retrieves cached parsed HTML for the given markdown content.
 * @param content - Raw markdown string
 * @returns Cached HTML string or undefined if not found
 */
export function getCachedParse(content: string): string | undefined {
  return parseCache.get(content);
}

/**
 * Stores parsed HTML in the cache with LRU eviction.
 * @param content - Raw markdown string (cache key)
 * @param html - Parsed HTML string (cache value)
 */
export function setCachedParse(content: string, html: string): void {
  // Implement LRU: if cache is full, remove oldest entry (first key)
  if (parseCache.size >= MAX_CACHE_SIZE) {
    const firstKey = parseCache.keys().next().value;
    if (firstKey !== undefined) {
      parseCache.delete(firstKey);
    }
  }

  parseCache.set(content, html);
}

/**
 * Clears the entire parse cache.
 * Useful for testing or memory management.
 */
export function clearParseCache(): void {
  parseCache.clear();
}

/**
 * Returns the current size of the parse cache.
 * Useful for monitoring and debugging.
 */
export function getParseCacheSize(): number {
  return parseCache.size;
}
