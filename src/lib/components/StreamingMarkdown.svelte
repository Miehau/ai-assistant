<script lang="ts">
  import { renderMarkdown } from "$lib/utils/markdownRenderer";
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
