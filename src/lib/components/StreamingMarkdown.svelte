<script lang="ts">
  import { extractMarkdownSources, renderMarkdown, type MarkdownSourceLedger } from "$lib/utils/markdownRenderer";
  import { getCachedParse, setCachedParse } from "$lib/utils/markdownCache";
  import { parseMarkdownBlocks, type MarkdownBlock } from "$lib/utils/parseMarkdownBlocks";

  export let content: string;
  export let isStreaming: boolean = false;

  type RenderedMarkdownBlock = MarkdownBlock & { html: string };

  let blocks: MarkdownBlock[] = [];
  let committedBlocks: MarkdownBlock[] = [];
  let liveTailContent = "";
  let renderedCommittedBlocks: RenderedMarkdownBlock[] = [];
  let renderedLiveTailHtml = "";
  let sourceLedger: MarkdownSourceLedger = { sources: [], unresolvedCitations: [] };
  let visibleSources: MarkdownSourceLedger["sources"] = [];
  let hiddenSourceCount = 0;

  function escapeHtml(text: string): string {
    return text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  function renderBlock(text: string, options: { enableHighlight: boolean; useCache: boolean }): string {
    if (!text) return "";

    if (options.useCache) {
      const cached = getCachedParse(text);
      if (cached) return cached;
    }

    try {
      const html = renderMarkdown(text, { enableHighlight: options.enableHighlight });
      if (options.useCache) {
        setCachedParse(text, html);
      }
      return html;
    } catch (error) {
      console.error("Markdown rendering error:", error);
      return escapeHtml(text);
    }
  }

  $: blocks = parseMarkdownBlocks(content);
  $: committedBlocks = isStreaming ? blocks.slice(0, -1) : blocks;
  $: liveTailContent = isStreaming ? (blocks.at(-1)?.content ?? "") : "";
  $: renderedCommittedBlocks = committedBlocks.map((block) => ({
    ...block,
    html: renderBlock(block.content, { enableHighlight: true, useCache: true }),
  }));
  $: renderedLiveTailHtml = isStreaming && liveTailContent
    ? renderBlock(liveTailContent, { enableHighlight: false, useCache: false })
    : "";
  $: sourceLedger = extractMarkdownSources(content);
  $: visibleSources = sourceLedger.sources.slice(0, 12);
  $: hiddenSourceCount = Math.max(0, sourceLedger.sources.length - visibleSources.length);
</script>

{#each renderedCommittedBlocks as block (block.id)}
  <div class="markdown-block" data-md-block={block.id}>
    {@html block.html}
  </div>
{/each}

{#if isStreaming && liveTailContent}
  <div class="markdown-block markdown-live-tail" data-md-live="true">
    {#if renderedLiveTailHtml}
      {@html renderedLiveTailHtml}
    {:else}
      <div style="white-space: pre-wrap;">{liveTailContent}</div>
    {/if}
  </div>
{/if}

{#if !isStreaming && (visibleSources.length > 0 || sourceLedger.unresolvedCitations.length > 0)}
  <div class="source-ledger" aria-label="Sources">
    {#if visibleSources.length > 0}
      <div class="source-ledger-title">Sources</div>
      <div class="source-ledger-list">
        {#each visibleSources as source (source.url)}
          <a
            class="source-ledger-item"
            href={source.url}
            target="_blank"
            rel="noopener noreferrer"
          >
            <span class="source-ledger-label">{source.label}</span>
            <span class="source-ledger-domain">{source.domain}</span>
          </a>
        {/each}
      </div>
      {#if hiddenSourceCount > 0}
        <div class="source-ledger-more">+{hiddenSourceCount} more sources</div>
      {/if}
    {/if}

    {#if sourceLedger.unresolvedCitations.length > 0 && visibleSources.length === 0}
      <div class="source-ledger-warning">
        Provider citations were omitted because the response only included internal IDs, not URLs.
      </div>
    {/if}
  </div>
{/if}
